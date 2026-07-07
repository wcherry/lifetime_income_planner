use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;
use serde_json::json;

/// Application-wide error type. Every variant maps to an HTTP status code and a
/// JSON body of the shape `{ "error": "message" }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("Invalid email or password")]
    InvalidCredentials,

    #[error("Authentication required")]
    Unauthorized,

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    NotFound(String),

    #[error("Internal server error")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::InvalidCredentials | AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Log the underlying cause of 500s but never leak it to the client.
        if let AppError::Internal(detail) = self {
            log::error!("internal error: {detail}");
        }
        HttpResponse::build(self.status_code()).json(json!(ErrorBody {
            error: self.to_string(),
        }))
    }
}

// Convenience conversions so handlers can use `?`.
impl From<diesel::result::Error> for AppError {
    fn from(err: diesel::result::Error) -> Self {
        match err {
            diesel::result::Error::NotFound => AppError::NotFound("Resource not found".into()),
            other => AppError::Internal(other.to_string()),
        }
    }
}

impl From<r2d2::Error> for AppError {
    fn from(err: r2d2::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
