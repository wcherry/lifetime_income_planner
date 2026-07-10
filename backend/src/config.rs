use std::env;

/// Runtime configuration loaded from environment variables (see `.env`).
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub host: String,
    pub port: u16,
    /// Plaid sandbox/production credentials (Phase 6: account aggregation).
    /// Left unset in local dev — endpoints that need them return a clear
    /// "not configured" error rather than panicking at startup.
    pub plaid_client_id: Option<String>,
    pub plaid_secret: Option<String>,
    pub plaid_env: String,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "lifetime_income_planner.db".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set in the environment"),
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24),
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            plaid_client_id: env::var("PLAID_CLIENT_ID").ok().filter(|s| !s.is_empty()),
            plaid_secret: env::var("PLAID_SECRET").ok().filter(|s| !s.is_empty()),
            plaid_env: env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string()),
        }
    }
}
