use actix_web::{get, web, HttpResponse};
use chrono::{Datelike, Utc};
use diesel::prelude::*;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::aca::AcaTables;
use crate::irmaa::IrmaaTables;
use crate::models::aca::load_aca_tables;
use crate::models::irmaa::load_irmaa_tables;
use crate::models::tax::load_tax_tables;
use crate::models::{
    Account, Assumptions, IncomeSource, LifeEvent, Profile, ProjectionResponse, SpendingItem,
    DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM, DEFAULT_HEALTHCARE_INFLATION_RATE, DEFAULT_INFLATION_RATE,
    DEFAULT_INVESTMENT_RETURN_RATE, DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM,
    DEFAULT_ROTH_CONVERSION_CEILING, DEFAULT_SOCIAL_SECURITY_COLA_RATE, DEFAULT_WITHDRAWAL_STRATEGY,
};
use crate::projection::{run_projection, ProjectionInputs};
use crate::schema::{accounts, assumptions, income_sources, life_events, profiles, spending_items};
use crate::tax::TaxTables;

/// All of a user's planning data loaded together for a projection.
struct PlanningData {
    profile: Option<Profile>,
    accounts: Vec<Account>,
    income: Vec<IncomeSource>,
    spending: Vec<SpendingItem>,
    life_events: Vec<LifeEvent>,
    assumptions: Option<Assumptions>,
    tax_tables: TaxTables,
    aca_tables: AcaTables,
    irmaa_tables: IrmaaTables,
}

/// Load a user's planning data and run the projection engine. Shared by the
/// JSON projection endpoint and the CSV tax-report export (Phase 2, feature 8).
pub(crate) async fn build_projection(pool: &DbPool, user_id: String) -> AppResult<ProjectionResponse> {
    let pool = pool.clone();
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
            tax_tables: load_tax_tables(&mut conn)?,
            aca_tables: load_aca_tables(&mut conn)?,
            irmaa_tables: load_irmaa_tables(&mut conn)?,
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
    let (
        inflation_rate,
        investment_return_rate,
        healthcare_inflation_rate,
        social_security_cola_rate,
        roth_conversion_ceiling,
        roth_conversion_start_year,
        roth_conversion_end_year,
        withdrawal_strategy,
        aca_benchmark_annual_premium,
        medicare_part_b_annual_premium,
    ) = match &data.assumptions {
        Some(a) => (
            a.inflation_rate,
            a.investment_return_rate,
            a.healthcare_inflation_rate,
            a.social_security_cola_rate,
            a.roth_conversion_ceiling,
            a.roth_conversion_start_year,
            a.roth_conversion_end_year,
            a.withdrawal_strategy.clone(),
            a.aca_benchmark_annual_premium,
            a.medicare_part_b_annual_premium,
        ),
        None => (
            DEFAULT_INFLATION_RATE,
            DEFAULT_INVESTMENT_RETURN_RATE,
            DEFAULT_HEALTHCARE_INFLATION_RATE,
            DEFAULT_SOCIAL_SECURITY_COLA_RATE,
            DEFAULT_ROTH_CONVERSION_CEILING,
            None,
            None,
            DEFAULT_WITHDRAWAL_STRATEGY.to_string(),
            DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM,
            DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM,
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
        roth_conversion_ceiling,
        roth_conversion_start_year,
        roth_conversion_end_year,
        withdrawal_strategy,
        aca_benchmark_annual_premium,
        medicare_part_b_annual_premium,
        tax_tables: data.tax_tables,
        aca_tables: data.aca_tables,
        irmaa_tables: data.irmaa_tables,
    };

    Ok(run_projection(&inputs))
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
    let projection = build_projection(&pool, auth.user_id.clone()).await?;
    Ok(HttpResponse::Ok().json(projection))
}
