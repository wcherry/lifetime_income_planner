use actix_web::{get, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{Assumptions, AssumptionsRequest, AssumptionsResponse, NewAssumptions};
use crate::schema::assumptions;

/// Fetch the authenticated user's planning assumptions.
///
/// Unlike most resources, this always returns a value: if the user has not
/// saved any assumptions yet, sensible defaults are returned with
/// `is_default: true`.
#[utoipa::path(
    get,
    path = "/api/assumptions",
    tag = "assumptions",
    responses(
        (status = 200, description = "The user's assumptions (or defaults)", body = AssumptionsResponse),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/assumptions")]
pub async fn get_assumptions(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let row = web::block(move || -> AppResult<Option<Assumptions>> {
        let mut conn = pool.get()?;
        let row = assumptions::table
            .filter(assumptions::user_id.eq(&auth.user_id))
            .select(Assumptions::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(row)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body = row
        .map(AssumptionsResponse::from)
        .unwrap_or_else(AssumptionsResponse::defaults);
    Ok(HttpResponse::Ok().json(body))
}

/// Create or replace the authenticated user's planning assumptions.
#[utoipa::path(
    put,
    path = "/api/assumptions",
    tag = "assumptions",
    request_body = AssumptionsRequest,
    responses(
        (status = 200, description = "Assumptions saved", body = AssumptionsResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/assumptions")]
pub async fn upsert_assumptions(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<AssumptionsRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let user_id = auth.user_id.clone();
    let new_row = NewAssumptions {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.clone(),
        inflation_rate: payload.inflation_rate,
        investment_return_rate: payload.investment_return_rate,
        healthcare_inflation_rate: payload.healthcare_inflation_rate,
        social_security_cola_rate: payload.social_security_cola_rate,
        updated_at: Utc::now().naive_utc(),
        roth_conversion_ceiling: payload.roth_conversion_ceiling,
        roth_conversion_start_year: payload.roth_conversion_start_year,
        roth_conversion_end_year: payload.roth_conversion_end_year,
        withdrawal_strategy: payload.withdrawal_strategy.as_str().to_string(),
        aca_benchmark_annual_premium: payload.aca_benchmark_annual_premium,
        medicare_part_b_annual_premium: payload.medicare_part_b_annual_premium,
    };

    let pool = pool.clone();
    let saved = web::block(move || -> AppResult<Assumptions> {
        let mut conn = pool.get()?;

        let existing_id: Option<String> = assumptions::table
            .filter(assumptions::user_id.eq(&user_id))
            .select(assumptions::id)
            .first(&mut conn)
            .optional()?;

        match existing_id {
            Some(id) => {
                diesel::update(assumptions::table.filter(assumptions::id.eq(&id)))
                    .set((
                        assumptions::inflation_rate.eq(new_row.inflation_rate),
                        assumptions::investment_return_rate.eq(new_row.investment_return_rate),
                        assumptions::healthcare_inflation_rate
                            .eq(new_row.healthcare_inflation_rate),
                        assumptions::social_security_cola_rate
                            .eq(new_row.social_security_cola_rate),
                        assumptions::updated_at.eq(new_row.updated_at),
                        assumptions::roth_conversion_ceiling.eq(new_row.roth_conversion_ceiling),
                        assumptions::roth_conversion_start_year
                            .eq(new_row.roth_conversion_start_year),
                        assumptions::roth_conversion_end_year
                            .eq(new_row.roth_conversion_end_year),
                        assumptions::withdrawal_strategy.eq(new_row.withdrawal_strategy.clone()),
                        assumptions::aca_benchmark_annual_premium
                            .eq(new_row.aca_benchmark_annual_premium),
                        assumptions::medicare_part_b_annual_premium
                            .eq(new_row.medicare_part_b_annual_premium),
                    ))
                    .execute(&mut conn)?;
            }
            None => {
                diesel::insert_into(assumptions::table)
                    .values(&new_row)
                    .execute(&mut conn)?;
            }
        }

        let saved = assumptions::table
            .filter(assumptions::user_id.eq(&new_row.user_id))
            .select(Assumptions::as_select())
            .first(&mut conn)?;
        Ok(saved)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(AssumptionsResponse::from(saved)))
}
