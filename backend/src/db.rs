use diesel::r2d2::{self, ConnectionManager};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub type DbPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Build the r2d2 connection pool and enable foreign key enforcement,
/// which SQLite leaves off by default.
pub fn build_pool(database_url: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    r2d2::Pool::builder()
        .connection_customizer(Box::new(ForeignKeysOn))
        .build(manager)
        .expect("Failed to build database connection pool")
}

/// Apply any pending migrations at startup so the schema is always current.
pub fn run_migrations(pool: &DbPool) {
    let mut conn = pool.get().expect("Failed to get connection for migrations");
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run database migrations");
}

#[derive(Debug)]
struct ForeignKeysOn;

impl r2d2::CustomizeConnection<SqliteConnection, r2d2::Error> for ForeignKeysOn {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        use diesel::connection::SimpleConnection;
        conn.batch_execute("PRAGMA foreign_keys = ON;")
            .map_err(r2d2::Error::QueryError)
    }
}
