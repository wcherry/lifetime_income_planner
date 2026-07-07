use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand_core::OsRng;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::future::{ready, Ready};

use crate::config::Config;
use crate::error::AppError;

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

/// Extractor that authenticates the request from the `Authorization: Bearer`
/// header and exposes the authenticated user's id to handlers.
pub struct AuthUser {
    pub user_id: String,
}

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(extract_auth_user(req))
    }
}

fn extract_auth_user(req: &HttpRequest) -> Result<AuthUser, AppError> {
    let config = req
        .app_data::<web::Data<Config>>()
        .ok_or_else(|| AppError::Internal("missing config in app data".into()))?;

    let header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or(AppError::Unauthorized)?
        .trim();

    let claims = decode_token(token, &config.jwt_secret)?;
    Ok(AuthUser {
        user_id: claims.sub,
    })
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
        };
        let token = generate_token("user-123", &config).unwrap();
        let claims = decode_token(&token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "user-123");
    }
}
