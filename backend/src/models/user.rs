use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::users;

/// Persisted user row. `password_hash` never leaves the backend.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub id: String,
    pub email: String,
    pub password_hash: String,
}

/// Public view of a user returned by the API.
#[derive(Serialize, ToSchema)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        UserResponse {
            id: u.id,
            email: u.email,
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct RegisterRequest {
    #[validate(email(message = "must be a valid email address"))]
    #[schema(example = "jane@example.com")]
    pub email: String,

    #[validate(length(min = 8, message = "must be at least 8 characters"))]
    #[schema(example = "correcthorsebattery")]
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "must be a valid email address"))]
    #[schema(example = "jane@example.com")]
    pub email: String,

    #[validate(length(min = 1, message = "password is required"))]
    #[schema(example = "correcthorsebattery")]
    pub password: String,
}

/// Response returned by register and login: a bearer token plus the user.
#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
}
