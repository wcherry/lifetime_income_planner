use actix_web::{dev::Payload, http::Method, web, FromRequest, HttpRequest};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand_core::OsRng;
use chrono::{Duration, Utc};
use diesel::prelude::*;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::Collaborator;
use crate::schema::collaborators;

/// Hash a plaintext password with Argon2id.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
}

/// Verify a plaintext password against a stored Argon2 hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// JWT claims. `sub` holds the user id.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: i64,
    pub exp: i64,
}

/// Issue a signed JWT for the given user.
pub fn generate_token(user_id: &str, config: &Config) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::hours(config.jwt_expiry_hours)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("token generation failed: {e}")))
}

fn decode_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized)
}

/// Header a client sets to act on another owner's data via a granted
/// collaborator context (roadmap Phase 6, feature 7). Absent or equal to the
/// caller's own id means "act as myself."
const CONTEXT_HEADER: &str = "X-Context-User";

/// The caller's relationship to the data `AuthUser::user_id` refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRole {
    /// Acting on their own data.
    Owner,
    /// Acting on a spouse's data via a full read-write grant.
    Spouse,
    /// Acting on an owner's data via a read-only advisor grant.
    Advisor,
}

impl AccessRole {
    pub fn is_read_only(&self) -> bool {
        matches!(self, AccessRole::Advisor)
    }
}

/// Extractor that authenticates the request from the `Authorization: Bearer`
/// header. `user_id` is whose data the request operates on — the caller's
/// own id by default, or another owner's id when the caller has an active
/// collaborator grant and sends the `X-Context-User` header. Every existing
/// handler that filters by `auth.user_id` therefore honors collaboration
/// automatically, with no per-handler changes: this extractor is the single
/// point where access is checked and advisor writes are rejected.
pub struct AuthUser {
    pub user_id: String,
    /// The authenticated identity from the JWT, regardless of active context.
    pub caller_id: String,
    pub role: AccessRole,
}

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let config = req.app_data::<web::Data<Config>>().cloned();
        let pool = req.app_data::<web::Data<DbPool>>().cloned();
        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        let context_user = req
            .headers()
            .get(CONTEXT_HEADER)
            .and_then(|h| h.to_str().ok())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let is_mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);

        Box::pin(async move {
            let config = config
                .ok_or_else(|| AppError::Internal("missing config in app data".into()))?;
            let pool =
                pool.ok_or_else(|| AppError::Internal("missing db pool in app data".into()))?;

            let header = auth_header.ok_or(AppError::Unauthorized)?;
            let token = header
                .strip_prefix("Bearer ")
                .or_else(|| header.strip_prefix("bearer "))
                .ok_or(AppError::Unauthorized)?
                .trim();
            let claims = decode_token(token, &config.jwt_secret)?;
            let caller_id = claims.sub;

            let target_user_id = context_user.unwrap_or_else(|| caller_id.clone());
            if target_user_id == caller_id {
                return Ok(AuthUser {
                    user_id: caller_id.clone(),
                    caller_id,
                    role: AccessRole::Owner,
                });
            }

            let owner_id = target_user_id.clone();
            let collaborator_id = caller_id.clone();
            let grant = web::block(move || -> Result<Option<Collaborator>, AppError> {
                let mut conn = pool.get()?;
                let row = collaborators::table
                    .filter(collaborators::owner_user_id.eq(&owner_id))
                    .filter(collaborators::collaborator_user_id.eq(&collaborator_id))
                    .filter(collaborators::status.eq("active"))
                    .select(Collaborator::as_select())
                    .first(&mut conn)
                    .optional()?;
                Ok(row)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??;

            let Some(grant) = grant else {
                return Err(AppError::Forbidden(
                    "You don't have access to this account".into(),
                ));
            };

            let role = if grant.role == "advisor" {
                AccessRole::Advisor
            } else {
                AccessRole::Spouse
            };
            if role.is_read_only() && is_mutating {
                return Err(AppError::Forbidden(
                    "Advisors have read-only access".into(),
                ));
            }

            Ok(AuthUser {
                user_id: target_user_id,
                caller_id,
                role,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trips() {
        let hash = hash_password("supersecret").unwrap();
        assert!(verify_password("supersecret", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn token_round_trips() {
        let config = Config {
            database_url: "x".into(),
            jwt_secret: "test-secret".into(),
            jwt_expiry_hours: 1,
            host: "127.0.0.1".into(),
            port: 8080,
            plaid_client_id: None,
            plaid_secret: None,
            plaid_env: "sandbox".into(),
        };
        let token = generate_token("user-123", &config).unwrap();
        let claims = decode_token(&token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "user-123");
    }
}
