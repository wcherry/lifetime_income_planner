//! Quarterly review workflow (roadmap Phase 5, features 1-4): a user enters
//! actual income/spending/tax and each account's actual ending balance for a
//! quarter, compared against the plan's previously-projected figures for that
//! same quarter. Completing a review immediately overwrites live account
//! balances with the entered actuals — this *is* the "automatic
//! recalculation," there is no separate apply step. A review only exists
//! once completed; "what needs review" is computed live each request by
//! diffing today's date against what's already been completed this year.

use std::collections::HashSet;

use actix_web::{get, post, web, HttpResponse};
use chrono::{Datelike, Utc};
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::handlers::projection::build_projection;
use crate::models::{
    Account, ActualAccountBalanceInput, CompleteReviewRequest, DueAccountBalance,
    DueQuarterlyReview, NewQuarterlyReview, QuarterlyReview, QuarterlyReviewOverview,
    QuarterlyReviewResponse, ReviewAccountBalance,
};
use crate::reconciliation;
use crate::schema::{accounts, quarterly_reviews};

/// List quarters still needing review for the current year, and the full
/// history of already-completed reviews (roadmap Phase 5, feature 1).
#[utoipa::path(
    get,
    path = "/api/quarterly-reviews",
    tag = "quarterly_review",
    responses(
        (status = 200, description = "Quarters due for review and the completed review history", body = QuarterlyReviewOverview),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/quarterly-reviews")]
pub async fn list_quarterly_reviews(
    pool: web::Data<DbPool>,
    auth: AuthUser,
) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let pool_clone = pool.clone();
    let (review_rows, account_rows) =
        web::block(move || -> AppResult<(Vec<QuarterlyReview>, Vec<Account>)> {
            let mut conn = pool_clone.get()?;
            let review_rows = quarterly_reviews::table
                .filter(quarterly_reviews::user_id.eq(&user_id))
                .order((
                    quarterly_reviews::year.desc(),
                    quarterly_reviews::quarter.desc(),
                ))
                .select(QuarterlyReview::as_select())
                .load(&mut conn)?;
            let account_rows = accounts::table
                .filter(accounts::user_id.eq(&user_id))
                .select(Account::as_select())
                .load(&mut conn)?;
            Ok((review_rows, account_rows))
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

    let history: Vec<QuarterlyReviewResponse> = review_rows
        .iter()
        .map(QuarterlyReviewResponse::from_row)
        .collect::<AppResult<Vec<_>>>()?;

    // `build_projection` does its own internal `web::block`, so it's called
    // directly rather than nested inside the one above.
    let projection = build_projection(&pool, auth.user_id.clone()).await?;

    let now = Utc::now();
    let current_year = now.year();
    let current_quarter = reconciliation::quarter_of_month(now.month());

    let completed_this_year: HashSet<i32> = review_rows
        .iter()
        .filter(|r| r.year == current_year)
        .map(|r| r.quarter)
        .collect();

    let mut due = Vec::new();
    for q in 1..=current_quarter {
        if completed_this_year.contains(&q) {
            continue;
        }
        let Some(qp) = projection
            .quarterly
            .iter()
            .find(|qp| qp.year == current_year && qp.quarter == q)
        else {
            continue;
        };
        let accounts_due: Vec<DueAccountBalance> = account_rows
            .iter()
            .map(|a| DueAccountBalance {
                account_id: a.id.clone(),
                account_name: a.name.clone(),
                category: a.category.clone(),
                current_balance: a.current_balance,
            })
            .collect();
        due.push(DueQuarterlyReview {
            year: current_year,
            quarter: q,
            label: qp.label.clone(),
            is_current: q == current_quarter,
            planned_income: qp.income,
            planned_spending: qp.spending,
            planned_tax: qp.estimated_tax,
            planned_withdrawal: qp.total_withdrawal,
            accounts: accounts_due,
        });
    }

    Ok(HttpResponse::Ok().json(QuarterlyReviewOverview { due, history }))
}

/// Check payload account balances against the user's current accounts:
/// every payload id must correspond to a real account, no balance may be
/// negative, and every current account must be present (full coverage is
/// required since completing a review mutates live data). Extracted from the
/// handler so it's unit-testable without a database.
fn validate_actual_balances(
    payload: &[ActualAccountBalanceInput],
    accounts: &[Account],
) -> AppResult<Vec<ReviewAccountBalance>> {
    let mut result = Vec::with_capacity(payload.len());
    for entry in payload {
        if entry.ending_balance < 0.0 {
            return Err(AppError::BadRequest(format!(
                "Ending balance for account {} cannot be negative",
                entry.account_id
            )));
        }
        let account = accounts
            .iter()
            .find(|a| a.id == entry.account_id)
            .ok_or_else(|| {
                AppError::BadRequest(format!("Unknown account id: {}", entry.account_id))
            })?;
        result.push(ReviewAccountBalance {
            account_id: account.id.clone(),
            account_name: account.name.clone(),
            category: account.category.clone(),
            starting_balance: account.current_balance,
            ending_balance: entry.ending_balance,
        });
    }

    if payload.len() != accounts.len() {
        return Err(AppError::BadRequest(format!(
            "All {} current accounts must be included in the review; got {}",
            accounts.len(),
            payload.len()
        )));
    }

    Ok(result)
}

/// Complete a quarterly review (roadmap Phase 5, features 2-4): records the
/// entered actuals against the previously-planned figures for the quarter,
/// then immediately overwrites each account's `current_balance` with its
/// entered ending balance — the "automatic recalculation." One review per
/// `(user, year, quarter)`; re-submitting an already-reviewed quarter is a
/// conflict, and future quarters cannot be reviewed.
#[utoipa::path(
    post,
    path = "/api/quarterly-reviews/{year}/{quarter}/complete",
    tag = "quarterly_review",
    params(
        ("year" = i32, Path, description = "Calendar year of the quarter being reviewed"),
        ("quarter" = i32, Path, description = "Quarter number, 1 through 4"),
    ),
    request_body = CompleteReviewRequest,
    responses(
        (status = 201, description = "Review completed; account balances updated to the entered actuals", body = QuarterlyReviewResponse),
        (status = 400, description = "Validation error, an invalid/future quarter, or incomplete account coverage"),
        (status = 409, description = "This quarter has already been reviewed"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/quarterly-reviews/{year}/{quarter}/complete")]
pub async fn complete_quarterly_review(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<(i32, i32)>,
    body: web::Json<CompleteReviewRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let (year, quarter) = path.into_inner();
    if !(1..=4).contains(&quarter) {
        return Err(AppError::BadRequest(
            "Quarter must be between 1 and 4".into(),
        ));
    }

    let now = Utc::now();
    let current_year = now.year();
    let current_quarter = reconciliation::quarter_of_month(now.month());
    if year != current_year || quarter > current_quarter {
        return Err(AppError::BadRequest(
            "Cannot complete a review for a future quarter".into(),
        ));
    }

    let user_id = auth.user_id.clone();
    let projection = build_projection(&pool, user_id.clone()).await?;
    let quarter_projection = projection
        .quarterly
        .iter()
        .find(|q| q.year == year && q.quarter == quarter)
        .cloned()
        .ok_or_else(|| {
            AppError::Internal("current-year projection is missing the requested quarter".into())
        })?;

    let pool = pool.clone();
    let review = web::block(move || -> AppResult<QuarterlyReview> {
        let mut conn = pool.get()?;
        conn.transaction::<QuarterlyReview, AppError, _>(|conn| {
            let existing = quarterly_reviews::table
                .filter(quarterly_reviews::user_id.eq(&user_id))
                .filter(quarterly_reviews::year.eq(year))
                .filter(quarterly_reviews::quarter.eq(quarter))
                .select(QuarterlyReview::as_select())
                .first::<QuarterlyReview>(conn)
                .optional()?;
            if existing.is_some() {
                return Err(AppError::Conflict(format!(
                    "{year} Q{quarter} has already been reviewed"
                )));
            }

            let account_rows = accounts::table
                .filter(accounts::user_id.eq(&user_id))
                .select(Account::as_select())
                .load(conn)?;

            let balances = validate_actual_balances(&payload.actual_balances, &account_rows)?;
            let balances_json = serde_json::to_string(&balances).map_err(|e| {
                AppError::Internal(format!("failed to serialize actual balances: {e}"))
            })?;

            let id = Uuid::new_v4().to_string();
            let new_review = NewQuarterlyReview {
                id: id.clone(),
                user_id: user_id.clone(),
                year,
                quarter,
                planned_income: quarter_projection.income,
                planned_spending: quarter_projection.spending,
                planned_tax: quarter_projection.estimated_tax,
                planned_withdrawal: quarter_projection.total_withdrawal,
                actual_income: payload.actual_income,
                actual_spending: payload.actual_spending,
                actual_tax: payload.actual_tax,
                actual_balances: balances_json,
                notes: payload.notes.clone(),
                created_at: Utc::now().naive_utc(),
            };
            diesel::insert_into(quarterly_reviews::table)
                .values(&new_review)
                .execute(conn)?;

            // The actual "recalculation": overwrite each account's live
            // balance with the entered actual, in the same transaction as the
            // review insert.
            let updated_at = Utc::now().naive_utc();
            for entry in &payload.actual_balances {
                diesel::update(
                    accounts::table
                        .filter(accounts::id.eq(&entry.account_id))
                        .filter(accounts::user_id.eq(&user_id)),
                )
                .set((
                    accounts::current_balance.eq(entry.ending_balance),
                    accounts::updated_at.eq(updated_at),
                ))
                .execute(conn)?;
            }

            let review = quarterly_reviews::table
                .filter(quarterly_reviews::id.eq(&id))
                .select(QuarterlyReview::as_select())
                .first(conn)?;
            Ok(review)
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let response = QuarterlyReviewResponse::from_row(&review)?;
    Ok(HttpResponse::Created().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn sample_account(id: &str, balance: f64) -> Account {
        let now: NaiveDateTime = "2026-01-01T00:00:00"
            .parse()
            .expect("valid fixed timestamp");
        Account {
            id: id.to_string(),
            user_id: "user-1".to_string(),
            name: format!("Account {id}"),
            category: "taxable".to_string(),
            account_type: "brokerage".to_string(),
            owner: "self".to_string(),
            current_balance: balance,
            expected_roi: 5.0,
            dividend_yield: 0.0,
            cost_basis: None,
            allocation_stock_pct: None,
            allocation_bond_pct: None,
            allocation_cash_pct: None,
            withdrawal_restrictions: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn happy_path_covers_every_account_and_carries_starting_balances() {
        let accounts = vec![
            sample_account("a1", 100_000.0),
            sample_account("a2", 50_000.0),
        ];
        let payload = vec![
            ActualAccountBalanceInput {
                account_id: "a1".into(),
                ending_balance: 102_000.0,
            },
            ActualAccountBalanceInput {
                account_id: "a2".into(),
                ending_balance: 51_000.0,
            },
        ];

        let result = validate_actual_balances(&payload, &accounts).unwrap();

        assert_eq!(result.len(), 2);
        let a1 = result.iter().find(|b| b.account_id == "a1").unwrap();
        assert_eq!(a1.starting_balance, 100_000.0);
        assert_eq!(a1.ending_balance, 102_000.0);
        assert_eq!(a1.account_name, "Account a1");
        assert_eq!(a1.category, "taxable");
    }

    #[test]
    fn unknown_account_id_is_rejected() {
        let accounts = vec![sample_account("a1", 100_000.0)];
        let payload = vec![ActualAccountBalanceInput {
            account_id: "not-a-real-account".into(),
            ending_balance: 100_000.0,
        }];

        let result = validate_actual_balances(&payload, &accounts);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn missing_account_is_rejected_as_partial_coverage() {
        let accounts = vec![
            sample_account("a1", 100_000.0),
            sample_account("a2", 50_000.0),
        ];
        // Only one of the two accounts is included.
        let payload = vec![ActualAccountBalanceInput {
            account_id: "a1".into(),
            ending_balance: 100_000.0,
        }];

        let result = validate_actual_balances(&payload, &accounts);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn negative_ending_balance_is_rejected() {
        let accounts = vec![sample_account("a1", 100_000.0)];
        let payload = vec![ActualAccountBalanceInput {
            account_id: "a1".into(),
            ending_balance: -1.0,
        }];

        let result = validate_actual_balances(&payload, &accounts);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }
}
