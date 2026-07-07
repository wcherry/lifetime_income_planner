use actix_web::{get, web, HttpResponse};
use chrono::{Datelike, Utc};
use diesel::prelude::*;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    Account, Assumptions, IncomeSource, LifeEvent, Profile, SpendingItem, DEFAULT_HEALTHCARE_INFLATION_RATE,
    DEFAULT_INFLATION_RATE, DEFAULT_INVESTMENT_RETURN_RATE, DEFAULT_SOCIAL_SECURITY_COLA_RATE,
};
use crate::projection::{run_projection, ProjectionInputs};
use crate::schema::{accounts, assumptions, income_sources, life_events, profiles, spending_items};

/// All of a user's planning data loaded together for a projection.
struct PlanningData {
    profile: Option<Profile>,
    accounts: Vec<Account>,
    income: Vec<IncomeSource>,
    spending: Vec<SpendingItem>,
    life_events: Vec<LifeEvent>,
    assumptions: Option<Assumptions>,
}

/// Generate the retirement projection and near-term quarterly withdrawal
/// schedule for the authenticated user (roadmap Phase 1, features 8 & 9).
///
/// Requires a saved profile (it defines the projection horizon). Assumptions
/// fall back to system defaults when the user has not saved their own.
#[utoipa::path(
    get,
    path = "/api/projection",
    tag = "projection",
    responses(
        (status = 200, description = "The projection and quarterly withdrawal schedule", body = crate::models::ProjectionResponse),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/projection")]
pub async fn get_projection(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let user_id = auth.user_id.clone();

    let data = web::block(move || -> AppResult<PlanningData> {
        let mut conn = pool.get()?;
        Ok(PlanningData {
            profile: profiles::table
                .filter(profiles::user_id.eq(&user_id))
                .select(Profile::as_select())
                .first(&mut conn)
                .optional()?,
            accounts: accounts::table
                .filter(accounts::user_id.eq(&user_id))
                .select(Account::as_select())
                .load(&mut conn)?,
            income: income_sources::table
                .filter(income_sources::user_id.eq(&user_id))
                .select(IncomeSource::as_select())
                .load(&mut conn)?,
            spending: spending_items::table
                .filter(spending_items::user_id.eq(&user_id))
                .select(SpendingItem::as_select())
                .load(&mut conn)?,
            life_events: life_events::table
                .filter(life_events::user_id.eq(&user_id))
                .select(LifeEvent::as_select())
                .load(&mut conn)?,
            assumptions: assumptions::table
                .filter(assumptions::user_id.eq(&user_id))
                .select(Assumptions::as_select())
                .first(&mut conn)
                .optional()?,
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let profile = data.profile.ok_or_else(|| {
        AppError::BadRequest(
            "Create your retirement profile before generating a projection.".into(),
        )
    })?;

    // Use saved assumptions, or fall back to the same defaults the assumptions
    // endpoint serves.
    let assumptions_are_default = data.assumptions.is_none();
    let (inflation_rate, investment_return_rate, healthcare_inflation_rate, social_security_cola_rate) =
        match &data.assumptions {
            Some(a) => (
                a.inflation_rate,
                a.investment_return_rate,
                a.healthcare_inflation_rate,
                a.social_security_cola_rate,
            ),
            None => (
                DEFAULT_INFLATION_RATE,
                DEFAULT_INVESTMENT_RETURN_RATE,
                DEFAULT_HEALTHCARE_INFLATION_RATE,
                DEFAULT_SOCIAL_SECURITY_COLA_RATE,
            ),
        };

    let inputs = ProjectionInputs {
        current_year: Utc::now().year(),
        profile: &profile,
        accounts: &data.accounts,
        income: &data.income,
        spending: &data.spending,
        life_events: &data.life_events,
        inflation_rate,
        investment_return_rate,
        healthcare_inflation_rate,
        social_security_cola_rate,
        assumptions_are_default,
    };

    let projection = run_projection(&inputs);
    Ok(HttpResponse::Ok().json(projection))
}
