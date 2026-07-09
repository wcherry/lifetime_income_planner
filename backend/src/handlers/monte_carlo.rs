use actix_web::{post, web, HttpResponse};
use chrono::{Datelike, Utc};
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::handlers::projection::load_projection_data;
use crate::models::MonteCarloRequest;
use crate::monte_carlo::run_monte_carlo;

/// Run a Monte Carlo simulation over the authenticated user's saved plan
/// (roadmap Phase 4, feature 6): perturbs each simulated year's investment
/// return by a random shock and reports the probability the plan's money
/// lasts the full horizon, plus percentile outcome bands.
///
/// Requires a saved profile (same precondition as `/projection`).
#[utoipa::path(
    post,
    path = "/api/monte-carlo",
    tag = "monte_carlo",
    request_body = MonteCarloRequest,
    responses(
        (status = 200, description = "Simulation results", body = crate::models::MonteCarloResult),
        (status = 400, description = "No profile has been created yet, or invalid request parameters"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/monte-carlo")]
pub async fn run_monte_carlo_endpoint(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<MonteCarloRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let data = load_projection_data(&pool, auth.user_id.clone()).await?;
    let current_year = Utc::now().year();

    // CPU-bound (no I/O): thousands of pure projection re-runs. Run it on the
    // blocking threadpool like the DB work above, so it doesn't stall the
    // async reactor for other requests.
    let result = web::block(move || {
        let inputs = data.inputs(current_year);
        run_monte_carlo(&inputs, payload.num_simulations, payload.volatility)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(HttpResponse::Ok().json(result))
}
