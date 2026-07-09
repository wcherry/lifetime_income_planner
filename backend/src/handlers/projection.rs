use actix_web::{get, post, web, HttpResponse};
use chrono::{Datelike, Months, Utc};
use diesel::prelude::*;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::aca::AcaTables;
use crate::irmaa::IrmaaTables;
use crate::handlers::auth::format_validation;
use crate::models::aca::load_aca_tables;
use crate::models::irmaa::load_irmaa_tables;
use crate::models::tax::load_tax_tables;
use crate::models::{
    Account, Assumptions, IncomeSource, LifeEvent, OptimizationCandidate, OptimizationGoal,
    OptimizeRequest, OptimizeResponse, PlanSnapshot, Profile, ProjectionResponse, ProjectionSummary,
    SpendingItem, WhatIfRequest, DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM,
    DEFAULT_HEALTHCARE_INFLATION_RATE, DEFAULT_INFLATION_RATE, DEFAULT_INVESTMENT_RETURN_RATE,
    DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM, DEFAULT_ROTH_CONVERSION_CEILING,
    DEFAULT_SOCIAL_SECURITY_COLA_RATE, DEFAULT_WITHDRAWAL_STRATEGY,
};
use crate::projection::{run_projection, run_projection_with_shocks, ProjectionInputs};
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

/// Owned planning data with assumption defaults already resolved — enough to
/// build a `ProjectionInputs` by borrowing. Shared by the JSON projection
/// endpoint, the CSV tax-report export, and the Monte Carlo endpoint so they
/// all run against identical inputs.
pub(crate) struct LoadedProjectionData {
    pub profile: Profile,
    pub accounts: Vec<Account>,
    pub income: Vec<IncomeSource>,
    pub spending: Vec<SpendingItem>,
    pub life_events: Vec<LifeEvent>,
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub assumptions_are_default: bool,
    pub roth_conversion_ceiling: f64,
    pub roth_conversion_start_year: Option<i32>,
    pub roth_conversion_end_year: Option<i32>,
    pub withdrawal_strategy: String,
    pub aca_benchmark_annual_premium: f64,
    pub medicare_part_b_annual_premium: f64,
    pub tax_tables: TaxTables,
    pub aca_tables: AcaTables,
    pub irmaa_tables: IrmaaTables,
}

impl LoadedProjectionData {
    pub(crate) fn inputs(&self, current_year: i32) -> ProjectionInputs<'_> {
        ProjectionInputs {
            current_year,
            profile: &self.profile,
            accounts: &self.accounts,
            income: &self.income,
            spending: &self.spending,
            life_events: &self.life_events,
            inflation_rate: self.inflation_rate,
            investment_return_rate: self.investment_return_rate,
            healthcare_inflation_rate: self.healthcare_inflation_rate,
            social_security_cola_rate: self.social_security_cola_rate,
            assumptions_are_default: self.assumptions_are_default,
            roth_conversion_ceiling: self.roth_conversion_ceiling,
            roth_conversion_start_year: self.roth_conversion_start_year,
            roth_conversion_end_year: self.roth_conversion_end_year,
            withdrawal_strategy: self.withdrawal_strategy.clone(),
            aca_benchmark_annual_premium: self.aca_benchmark_annual_premium,
            medicare_part_b_annual_premium: self.medicare_part_b_annual_premium,
            tax_tables: self.tax_tables.clone(),
            aca_tables: self.aca_tables.clone(),
            irmaa_tables: self.irmaa_tables.clone(),
        }
    }
}

/// Load a user's planning data and resolve assumption defaults. Shared by the
/// JSON projection endpoint, the CSV tax-report export (Phase 2, feature 8),
/// and the Monte Carlo endpoint (Phase 4, feature 6).
pub(crate) async fn load_projection_data(
    pool: &DbPool,
    user_id: String,
) -> AppResult<LoadedProjectionData> {
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

    Ok(LoadedProjectionData {
        profile,
        accounts: data.accounts,
        income: data.income,
        spending: data.spending,
        life_events: data.life_events,
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
    })
}

/// Build a runnable `LoadedProjectionData` from a saved plan's snapshot,
/// entirely in memory — no database access beyond the tax/ACA/IRMAA reference
/// tables passed in. Used by scenario comparison (roadmap Phase 4, feature 2)
/// so a saved scenario can be projected without destructively loading it into
/// the live working set the way `POST /plans/{id}/load` does. Row ids are
/// fabricated (the engine never reads them beyond labeling withdrawal lines).
pub(crate) fn loaded_projection_data_from_snapshot(
    snapshot: &PlanSnapshot,
    tax_tables: TaxTables,
    aca_tables: AcaTables,
    irmaa_tables: IrmaaTables,
) -> AppResult<LoadedProjectionData> {
    let now = Utc::now().naive_utc();
    let sp = snapshot
        .profile
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("has no profile saved".into()))?;
    let profile = Profile {
        id: "scenario-profile".into(),
        user_id: "scenario".into(),
        first_name: sp.first_name.clone(),
        last_name: sp.last_name.clone(),
        date_of_birth: sp.date_of_birth,
        marital_status: sp.marital_status.clone(),
        filing_status: sp.filing_status.clone(),
        state: sp.state.clone(),
        retirement_date: sp.retirement_date,
        life_expectancy: sp.life_expectancy,
        spouse_first_name: sp.spouse_first_name.clone(),
        spouse_last_name: sp.spouse_last_name.clone(),
        spouse_date_of_birth: sp.spouse_date_of_birth,
        spouse_life_expectancy: sp.spouse_life_expectancy,
        created_at: now,
        updated_at: now,
    };

    let accounts: Vec<Account> = snapshot
        .accounts
        .iter()
        .enumerate()
        .map(|(i, a)| Account {
            id: format!("scenario-account-{i}"),
            user_id: "scenario".into(),
            name: a.name.clone(),
            category: a.category.clone(),
            account_type: a.account_type.clone(),
            owner: a.owner.clone(),
            current_balance: a.current_balance,
            expected_roi: a.expected_roi,
            dividend_yield: a.dividend_yield,
            cost_basis: a.cost_basis,
            allocation_stock_pct: a.allocation_stock_pct,
            allocation_bond_pct: a.allocation_bond_pct,
            allocation_cash_pct: a.allocation_cash_pct,
            withdrawal_restrictions: a.withdrawal_restrictions.clone(),
            created_at: now,
            updated_at: now,
        })
        .collect();

    let income: Vec<IncomeSource> = snapshot
        .income
        .iter()
        .enumerate()
        .map(|(i, s)| IncomeSource {
            id: format!("scenario-income-{i}"),
            user_id: "scenario".into(),
            name: s.name.clone(),
            income_type: s.income_type.clone(),
            owner: s.owner.clone(),
            amount: s.amount,
            frequency: s.frequency.clone(),
            start_date: s.start_date,
            end_date: s.end_date,
            growth_rate: s.growth_rate,
            cola: s.cola,
            taxability: s.taxability.clone(),
            notes: s.notes.clone(),
            created_at: now,
            updated_at: now,
        })
        .collect();

    let spending: Vec<SpendingItem> = snapshot
        .spending
        .iter()
        .enumerate()
        .map(|(i, s)| SpendingItem {
            id: format!("scenario-spending-{i}"),
            user_id: "scenario".into(),
            name: s.name.clone(),
            category: s.category.clone(),
            amount: s.amount,
            frequency: s.frequency.clone(),
            inflation_adjusted: s.inflation_adjusted,
            start_year: s.start_year,
            end_year: s.end_year,
            notes: s.notes.clone(),
            created_at: now,
            updated_at: now,
        })
        .collect();

    let life_events: Vec<LifeEvent> = snapshot
        .life_events
        .iter()
        .enumerate()
        .map(|(i, e)| LifeEvent {
            id: format!("scenario-event-{i}"),
            user_id: "scenario".into(),
            name: e.name.clone(),
            event_type: e.event_type.clone(),
            event_date: e.event_date,
            direction: e.direction.clone(),
            amount: e.amount,
            taxable: e.taxable,
            inflation_adjusted: e.inflation_adjusted,
            recurrence: e.recurrence.clone(),
            end_date: e.end_date,
            notes: e.notes.clone(),
            created_at: now,
            updated_at: now,
        })
        .collect();

    let assumptions_are_default = snapshot.assumptions.is_none();
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
    ) = match &snapshot.assumptions {
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

    Ok(LoadedProjectionData {
        profile,
        accounts,
        income,
        spending,
        life_events,
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
        tax_tables,
        aca_tables,
        irmaa_tables,
    })
}

/// Load a user's planning data and run the projection engine. Shared by the
/// JSON projection endpoint and the CSV tax-report export (Phase 2, feature 8).
pub(crate) async fn build_projection(pool: &DbPool, user_id: String) -> AppResult<ProjectionResponse> {
    let data = load_projection_data(pool, user_id).await?;
    let inputs = data.inputs(Utc::now().year());
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

/// Recompute the projection with interactive "what-if" overrides layered on
/// the live working set (roadmap Phase 4, feature 3): inflation, per-account
/// investment return, overall spending level, Social Security claiming age,
/// and a one-time market crash. Nothing is saved — this is a scratch
/// recalculation for sliders in the UI to react to live.
///
/// Requires a saved profile, same as `GET /projection`.
#[utoipa::path(
    post,
    path = "/api/projection/what-if",
    tag = "projection",
    request_body = WhatIfRequest,
    responses(
        (status = 200, description = "The recalculated projection with overrides applied", body = crate::models::ProjectionResponse),
        (status = 400, description = "No profile has been created yet, or invalid overrides"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/projection/what-if")]
pub async fn what_if_projection(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<WhatIfRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let mut data = load_projection_data(&pool, auth.user_id.clone()).await?;

    if let Some(rate) = payload.inflation_rate {
        data.inflation_rate = rate;
    }
    if let Some(delta) = payload.investment_return_delta {
        for acc in data.accounts.iter_mut() {
            acc.expected_roi += delta;
        }
    }
    if let Some(pct) = payload.spending_adjustment_pct {
        let factor = 1.0 + pct / 100.0;
        for item in data.spending.iter_mut() {
            item.amount *= factor;
        }
    }
    if let Some(years) = payload.social_security_delay_years {
        for inc in data.income.iter_mut() {
            if inc.income_type != "social_security" {
                continue;
            }
            let shifted = if years >= 0 {
                inc.start_date.checked_add_months(Months::new(years as u32 * 12))
            } else {
                inc.start_date
                    .checked_sub_months(Months::new((-years) as u32 * 12))
            };
            if let Some(d) = shifted {
                inc.start_date = d;
            }
        }
    }

    let inputs = data.inputs(Utc::now().year());
    let projection = match payload.market_crash_pct {
        Some(shock) => run_projection_with_shocks(&inputs, &[shock]),
        None => run_projection(&inputs),
    };

    Ok(HttpResponse::Ok().json(projection))
}

/// Search a small grid of withdrawal-strategy / Roth-conversion-ceiling
/// combinations against the live working set and rank them by the chosen
/// goal (roadmap Phase 4, feature 5). Reuses the same projection engine as
/// `GET /projection` and `POST /projection/what-if` as the evaluator —
/// nothing new is modeled here, this just automates the manual comparison a
/// user would otherwise do slider-by-slider.
///
/// Requires a saved profile, same as `GET /projection`.
#[utoipa::path(
    post,
    path = "/api/projection/optimize",
    tag = "projection",
    request_body = OptimizeRequest,
    responses(
        (status = 200, description = "Ranked candidate strategies, best first", body = crate::models::OptimizeResponse),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/projection/optimize")]
pub async fn optimize_projection(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<OptimizeRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    let mut data = load_projection_data(&pool, auth.user_id.clone()).await?;
    let current_year = Utc::now().year();

    let candidates = optimization_candidates(&mut data, current_year, payload.goal);

    Ok(HttpResponse::Ok().json(OptimizeResponse {
        goal: payload.goal,
        candidates,
    }))
}

/// The strategy/ceiling grid the optimizer searches (Phase 4, feature 5).
/// Roth ceilings are round dollar figures a user would recognize, not tied to
/// any particular tax bracket (those shift by year, filing status, and state).
const OPTIMIZE_STRATEGIES: [&str; 2] = ["conventional", "tax_optimized"];
const OPTIMIZE_ROTH_CEILINGS: [f64; 5] = [0.0, 25_000.0, 50_000.0, 75_000.0, 100_000.0];

fn optimization_candidates(
    data: &mut LoadedProjectionData,
    current_year: i32,
    goal: OptimizationGoal,
) -> Vec<OptimizationCandidate> {
    let mut candidates: Vec<OptimizationCandidate> =
        Vec::with_capacity(OPTIMIZE_STRATEGIES.len() * OPTIMIZE_ROTH_CEILINGS.len());
    for &strategy in &OPTIMIZE_STRATEGIES {
        for &ceiling in &OPTIMIZE_ROTH_CEILINGS {
            data.withdrawal_strategy = strategy.to_string();
            data.roth_conversion_ceiling = ceiling;
            data.roth_conversion_start_year = None;
            data.roth_conversion_end_year = None;

            let inputs = data.inputs(current_year);
            let projection = run_projection(&inputs);
            let score = score_for_goal(goal, &projection.summary);
            candidates.push(OptimizationCandidate {
                withdrawal_strategy: strategy.to_string(),
                roth_conversion_ceiling: ceiling,
                summary: projection.summary,
                score,
                recommended: false,
            });
        }
    }

    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(best) = candidates.first_mut() {
        best.recommended = true;
    }
    candidates
}

/// Goal-specific score for one candidate (Phase 4, feature 5). Always
/// "higher is better" regardless of goal, so ranking is a single sort.
fn score_for_goal(goal: OptimizationGoal, summary: &ProjectionSummary) -> f64 {
    match goal {
        OptimizationGoal::MinimizeTaxes => -summary.total_lifetime_taxes,
        OptimizationGoal::MaximizeEstate => summary.projected_ending_balance,
        OptimizationGoal::MaximizePlanLongevity => match summary.depletion_year {
            None => f64::MAX,
            Some(y) => y as f64,
        },
        OptimizationGoal::MinimizeIrmaa => -summary.total_lifetime_irmaa_surcharges,
        OptimizationGoal::MaximizeAcaSubsidy => summary.total_lifetime_aca_subsidies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountSnapshot, ProfileSnapshot};
    use chrono::NaiveDate;

    fn snapshot_with_one_account() -> PlanSnapshot {
        PlanSnapshot {
            profile: Some(ProfileSnapshot {
                first_name: "Pat".into(),
                last_name: "Saver".into(),
                date_of_birth: NaiveDate::from_ymd_opt(1960, 1, 1).unwrap(),
                marital_status: "single".into(),
                filing_status: "single".into(),
                state: "TX".into(),
                retirement_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                life_expectancy: 90,
                spouse_first_name: None,
                spouse_last_name: None,
                spouse_date_of_birth: None,
                spouse_life_expectancy: None,
            }),
            assumptions: None,
            accounts: vec![AccountSnapshot {
                name: "Brokerage".into(),
                category: "taxable".into(),
                account_type: "brokerage".into(),
                owner: "self".into(),
                current_balance: 100_000.0,
                expected_roi: 5.0,
                dividend_yield: 0.0,
                cost_basis: Some(100_000.0),
                allocation_stock_pct: None,
                allocation_bond_pct: None,
                allocation_cash_pct: None,
                withdrawal_restrictions: None,
            }],
            income: vec![],
            spending: vec![],
            life_events: vec![],
        }
    }

    #[test]
    fn snapshot_without_a_profile_is_rejected() {
        let snapshot = PlanSnapshot::default();
        let result = loaded_projection_data_from_snapshot(
            &snapshot,
            TaxTables::default_2025(),
            AcaTables::default_2025(),
            IrmaaTables::default_2025(),
        );
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn snapshot_data_carries_over_and_defaults_apply_when_assumptions_are_missing() {
        let snapshot = snapshot_with_one_account();
        let data = loaded_projection_data_from_snapshot(
            &snapshot,
            TaxTables::default_2025(),
            AcaTables::default_2025(),
            IrmaaTables::default_2025(),
        )
        .unwrap();

        assert_eq!(data.profile.first_name, "Pat");
        assert_eq!(data.accounts.len(), 1);
        assert_eq!(data.accounts[0].current_balance, 100_000.0);
        assert_eq!(data.accounts[0].expected_roi, 5.0);
        assert!(data.assumptions_are_default);
        assert_eq!(data.inflation_rate, DEFAULT_INFLATION_RATE);
        assert_eq!(data.investment_return_rate, DEFAULT_INVESTMENT_RETURN_RATE);

        // The result should be runnable through the projection engine.
        let inputs = data.inputs(2026);
        let projection = run_projection(&inputs);
        assert_eq!(projection.summary.current_net_worth, 100_000.0);
    }

    #[test]
    fn optimize_ranks_candidates_and_flags_exactly_one_recommended() {
        let snapshot = snapshot_with_one_account();
        let mut data = loaded_projection_data_from_snapshot(
            &snapshot,
            TaxTables::default_2025(),
            AcaTables::default_2025(),
            IrmaaTables::default_2025(),
        )
        .unwrap();

        let candidates = optimization_candidates(&mut data, 2026, OptimizationGoal::MaximizeEstate);

        assert_eq!(
            candidates.len(),
            OPTIMIZE_STRATEGIES.len() * OPTIMIZE_ROTH_CEILINGS.len()
        );
        let recommended_count = candidates.iter().filter(|c| c.recommended).count();
        assert_eq!(recommended_count, 1);
        assert!(candidates[0].recommended);

        // Sorted best-first: score is non-increasing down the list.
        for pair in candidates.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }

    #[test]
    fn optimize_score_matches_the_goals_direction() {
        let snapshot = snapshot_with_one_account();
        let mut data = loaded_projection_data_from_snapshot(
            &snapshot,
            TaxTables::default_2025(),
            AcaTables::default_2025(),
            IrmaaTables::default_2025(),
        )
        .unwrap();

        let candidates = optimization_candidates(&mut data, 2026, OptimizationGoal::MinimizeTaxes);
        let best_taxes = candidates[0].summary.total_lifetime_taxes;
        assert!(candidates
            .iter()
            .all(|c| best_taxes <= c.summary.total_lifetime_taxes));
    }
}
