use diesel::r2d2::{self, ConnectionManager};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub type DbPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Build the r2d2 connection pool and apply the per-connection pragmas
/// SQLite needs (see `SqlitePragmas`). Capped at a single connection: SQLite
/// allows only one writer at a time, and in practice a multi-connection pool
/// against one SQLite file still produces "database is locked" errors under
/// concurrent writes even with `busy_timeout` set — a DEFERRED transaction
/// that starts by reading, then tries to upgrade to a write lock, can lose
/// that race to another connection without a retry ever resolving it. With
/// one connection, every request is naturally serialized through r2d2's own
/// checkout queue instead of racing inside SQLite, which sidesteps the
/// problem entirely. That's a fine trade for this app's scale (no read
/// concurrency across requests, but each request is already fast).
pub fn build_pool(database_url: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    r2d2::Pool::builder()
        .max_size(1)
        .connection_customizer(Box::new(SqlitePragmas))
        .build(manager)
        .expect("Failed to build database connection pool")
}

/// Apply any pending migrations at startup so the schema is always current.
pub fn run_migrations(pool: &DbPool) {
    let mut conn = pool.get().expect("Failed to get connection for migrations");
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run database migrations");
}

/// Seed reference data the app depends on (currently the tax tables) if it is
/// missing. Idempotent, so existing (possibly admin-edited) data is preserved.
pub fn seed_reference_data(pool: &DbPool) {
    let mut conn = pool.get().expect("Failed to get connection for seeding");
    crate::models::seed_tax_tables_if_empty(&mut conn).expect("Failed to seed tax tables");
    crate::models::seed_aca_tables_if_empty(&mut conn).expect("Failed to seed ACA tables");
    crate::models::seed_irmaa_brackets_if_empty(&mut conn).expect("Failed to seed IRMAA brackets");
    crate::models::seed_spending_tracker_categories_if_empty(&mut conn)
        .expect("Failed to seed spending tracker categories");
}

/// Per-connection setup applied to every connection the pool hands out.
/// Order matters here: `busy_timeout` is set *first* so it's already active
/// for the pragmas that follow — in particular, switching `journal_mode` to
/// WAL itself needs to briefly acquire an exclusive lock, and at pool
/// startup several connections can be created and race to do that switch
/// concurrently. With `busy_timeout` applied afterward instead, that race
/// surfaced as "database is locked" coming from connection acquisition
/// itself, before any request-level retry logic could help.
/// - `busy_timeout`: SQLite allows only one writer at a time. Without this,
///   a connection that tries to write while another write transaction is in
///   flight fails immediately with "database is locked" (SQLITE_BUSY);
///   this makes it retry for up to 5s before giving up, comfortably
///   covering this app's write load (e.g. bulk-categorize touching several
///   transactions back to back) instead of surfacing spurious lock errors.
/// - `journal_mode = WAL`: lets readers proceed concurrently with a writer,
///   instead of a writer blocking the whole file for the length of its
///   transaction (the default rollback-journal behavior).
/// - `foreign_keys`: SQLite leaves FK enforcement off by default.
#[derive(Debug)]
struct SqlitePragmas;

impl r2d2::CustomizeConnection<SqliteConnection, r2d2::Error> for SqlitePragmas {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        use diesel::connection::SimpleConnection;
        conn.batch_execute(
            "PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
        )
        .map_err(r2d2::Error::QueryError)
    }
}
