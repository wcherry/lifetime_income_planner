//! Personalized insights & anomaly detection (roadmap Phase 6, feature 6):
//! assembles an `InsightContext` from the projection, accounts, and
//! quarterly review history, then runs the pure rules in `crate::insights`.

use actix_web::{get, web, HttpResponse};
use chrono::{Datelike, Utc};
use diesel::prelude::*;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::projection::{build_projection, load_projection_data};
use crate::insights::{generate_insights, Insight, InsightContext, ReviewVariance};
use crate::models::{Account, ProjectionResponse, QuarterlyReview, QuarterlyReviewResponse};
use crate::monte_carlo::run_monte_carlo;
use crate::reconciliation;
use crate::schema::{accounts, quarterly_reviews};

/// Lightweight simulation used only to flag elevated sequence-of-return
/// risk — far fewer trials than the dedicated `/monte-carlo` endpoint, since
/// this runs on every insights request rather than on demand.
const INSIGHTS_MONTE_CARLO_SIMULATIONS: u32 = 200;
const INSIGHTS_MONTE_CARLO_VOLATILITY: f64 = 12.0;

/// Personalized insights and anomaly detection (roadmap Phase 6, feature 6):
/// ACA/IRMAA/RMD reminders, cash-flow and spending anomalies, an overdue
/// quarterly review, portfolio allocation, and sequence-of-return risk.
#[utoipa::path(
    get,
    path = "/api/insights",
    tag = "insights",
    responses(
        (status = 200, description = "Generated insights, highest severity first", body = [Insight]),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/insights")]
pub async fn list_insights(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let projection: ProjectionResponse = build_projection(&pool, user_id.clone()).await?;

    let now = Utc::now();
    let current_year = now.year();
    let current_quarter = reconciliation::quarter_of_month(now.month());

    let user_id_for_block = user_id.clone();
    let pool_clone = pool.clone();
    let (account_rows, review_rows) =
        web::block(move || -> AppResult<(Vec<Account>, Vec<QuarterlyReview>)> {
            let mut conn = pool_clone.get()?;
            let account_rows = accounts::table
                .filter(accounts::user_id.eq(&user_id_for_block))
                .select(Account::as_select())
                .load(&mut conn)?;
            let review_rows = quarterly_reviews::table
                .filter(quarterly_reviews::user_id.eq(&user_id_for_block))
                .order((
                    quarterly_reviews::year.desc(),
                    quarterly_reviews::quarter.desc(),
                ))
                .select(QuarterlyReview::as_select())
                .load(&mut conn)?;
            Ok((account_rows, review_rows))
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

    let review_due = !review_rows
        .iter()
        .any(|r| r.year == current_year && r.quarter == current_quarter);

    let latest_review = review_rows
        .first()
        .map(QuarterlyReviewResponse::from_row)
        .transpose()?
        .map(|r| ReviewVariance {
            label: r.label,
            planned_spending: r.planned_spending,
            spending_variance: r.spending_variance,
            net_cash_flow: r.reconciliation.net_cash_flow,
        });

    // Reused for both the retirement date and a light-weight Monte Carlo run
    // purely to gauge sequence-of-return risk; a failure here shouldn't block
    // the rest of the insights, since the projection above already succeeded.
    let (retirement_date, monte_carlo_success_rate) =
        match load_projection_data(&pool, user_id.clone()).await {
            Ok(data) => {
                let retirement_date = data.profile.retirement_date;
                let result = web::block(move || {
                    let inputs = data.inputs(current_year);
                    run_monte_carlo(
                        &inputs,
                        INSIGHTS_MONTE_CARLO_SIMULATIONS,
                        INSIGHTS_MONTE_CARLO_VOLATILITY,
                    )
                })
                .await
                .ok();
                (retirement_date, result.map(|r| r.success_rate))
            }
            Err(_) => (
                chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
                None,
            ),
        };

    let current_year_projection = projection.annual.iter().find(|y| y.year == current_year);
    let primary_age = current_year_projection
        .map(|y| y.primary_age)
        .unwrap_or(0);

    let ctx = InsightContext {
        current_year,
        primary_age,
        retirement_date,
        current_year_projection,
        accounts: &account_rows,
        review_due,
        latest_review,
        monte_carlo_success_rate,
    };

    let insights: Vec<Insight> = generate_insights(&ctx);
    Ok(HttpResponse::Ok().json(insights))
}
