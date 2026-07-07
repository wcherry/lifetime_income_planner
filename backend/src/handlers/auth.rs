use actix_web::{get, post, web, HttpResponse};
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::{generate_token, hash_password, verify_password, AuthUser};
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{AuthResponse, LoginRequest, NewUser, RegisterRequest, User, UserResponse};
use crate::schema::users;

/// Register a new user account.
#[utoipa::path(
    post,
    path = "/api/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created", body = AuthResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Email already registered"),
    )
)]
#[post("/auth/register")]
pub async fn register(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    body: web::Json<RegisterRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let email = payload.email.trim().to_lowercase();
    let password_hash = hash_password(&payload.password)?;

    let new_user = NewUser {
        id: Uuid::new_v4().to_string(),
        email: email.clone(),
        password_hash,
    };

    let pool = pool.clone();
    let user = web::block(move || -> AppResult<User> {
        let mut conn = pool.get()?;

        let existing: i64 = users::table
            .filter(users::email.eq(&email))
            .count()
            .get_result(&mut conn)?;
        if existing > 0 {
            return Err(AppError::Conflict("Email already registered".into()));
        }

        diesel::insert_into(users::table)
            .values(&new_user)
            .execute(&mut conn)?;

        let user = users::table
            .filter(users::id.eq(&new_user.id))
            .select(User::as_select())
            .first(&mut conn)?;
        Ok(user)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let token = generate_token(&user.id, &config)?;
    Ok(HttpResponse::Created().json(AuthResponse {
        token,
        user: user.into(),
    }))
}

/// Authenticate and receive a bearer token.
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated", body = AuthResponse),
        (status = 401, description = "Invalid credentials"),
    )
)]
#[post("/auth/login")]
pub async fn login(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    body: web::Json<LoginRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let email = payload.email.trim().to_lowercase();

    let pool = pool.clone();
    let user = web::block(move || -> AppResult<Option<User>> {
        let mut conn = pool.get()?;
        let user = users::table
            .filter(users::email.eq(&email))
            .select(User::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(user)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    // Verify against the stored hash. Treat a missing user the same as a bad
    // password so we don't leak which emails exist.
    let user = user.ok_or(AppError::InvalidCredentials)?;
    if !verify_password(&payload.password, &user.password_hash) {
        return Err(AppError::InvalidCredentials);
    }

    let token = generate_token(&user.id, &config)?;
    Ok(HttpResponse::Ok().json(AuthResponse {
        token,
        user: user.into(),
    }))
}

/// Return the currently authenticated user.
#[utoipa::path(
    get,
    path = "/api/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current user", body = UserResponse),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/auth/me")]
pub async fn me(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let user = web::block(move || -> AppResult<User> {
        let mut conn = pool.get()?;
        let user = users::table
            .filter(users::id.eq(&auth.user_id))
            .select(User::as_select())
            .first(&mut conn)?;
        Ok(user)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(UserResponse::from(user)))
}

/// Flatten validator errors into a single readable message.
pub fn format_validation(errors: &validator::ValidationErrors) -> String {
    let mut parts = Vec::new();
    for (field, errs) in errors.field_errors() {
        for e in errs {
            let msg = e
                .message
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "is invalid".to_string());
            parts.push(format!("{field} {msg}"));
        }
    }
    if parts.is_empty() {
        "Validation failed".to_string()
    } else {
        parts.join("; ")
    }
}
