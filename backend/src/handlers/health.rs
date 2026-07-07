use actix_web::{get, HttpResponse};
use serde_json::json;

/// Simple liveness probe.
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "health",
    responses((status = 200, description = "Service is healthy"))
)]
#[get("/health")]
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(json!({ "status": "ok" }))
}
