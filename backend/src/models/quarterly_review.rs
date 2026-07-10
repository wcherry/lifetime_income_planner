//! Quarterly review models (roadmap Phase 5, features 1-4): a user's
//! entered actuals for a quarter (income, spending, tax, and each account's
//! ending balance) compared against the projection's previously-planned
//! figures for that same quarter.

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::reconciliation::{self, AccountBalanceChange};
use crate::schema::quarterly_reviews;

/// One account's actual balance movement across a reviewed quarter, as stored
/// (JSON-encoded) in `quarterly_reviews.actual_balances`. `starting_balance`
/// is the account's live balance immediately before the review was applied —
/// captured at completion time because it's otherwise unrecoverable once
/// auto-apply overwrites the live balance with the ending value.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReviewAccountBalance {
    pub account_id: String,
    pub account_name: String,
    pub category: String,
    pub starting_balance: f64,
    pub ending_balance: f64,
}

/// A completed quarterly review row, as persisted.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = quarterly_reviews)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct QuarterlyReview {
    pub id: String,
    pub user_id: String,
    pub year: i32,
    pub quarter: i32,
    pub planned_income: f64,
    pub planned_spending: f64,
    pub planned_tax: f64,
    pub planned_withdrawal: f64,
    pub actual_income: f64,
    pub actual_spending: f64,
    pub actual_tax: f64,
    pub actual_balances: String,
    pub notes: Option<String>,
    pub created_at: NaiveDateTime,
}

/// Insertable row for a newly completed quarterly review.
#[derive(Insertable)]
#[diesel(table_name = quarterly_reviews)]
pub struct NewQuarterlyReview {
    pub id: String,
    pub user_id: String,
    pub year: i32,
    pub quarter: i32,
    pub planned_income: f64,
    pub planned_spending: f64,
    pub planned_tax: f64,
    pub planned_withdrawal: f64,
    pub actual_income: f64,
    pub actual_spending: f64,
    pub actual_tax: f64,
    pub actual_balances: String,
    pub notes: Option<String>,
    pub created_at: NaiveDateTime,
}

/// One account's actual ending balance, as submitted in a completion request.
/// No per-item validator here: full-coverage, unknown-id, and
/// non-negative-balance checks all happen together in the handler (a
/// `#[validate(nested)]` Vec plus a manual coverage check would duplicate the
/// same condition two different ways).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ActualAccountBalanceInput {
    pub account_id: String,
    pub ending_balance: f64,
}

/// Request body for completing a quarterly review.
///
/// Deliberately no min-length check on `actual_balances` — a user with zero
/// accounts must still be able to submit an empty array.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CompleteReviewRequest {
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub actual_income: f64,
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub actual_spending: f64,
    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub actual_tax: f64,
    pub actual_balances: Vec<ActualAccountBalanceInput>,
    #[validate(length(max = 2000))]
    pub notes: Option<String>,
}

/// API view of a reconciliation (derived at read time, never stored).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReconciliationSummary {
    pub total_starting_balance: f64,
    pub total_ending_balance: f64,
    pub net_balance_change: f64,
    pub net_cash_flow: f64,
    pub implied_investment_gain: f64,
}

impl From<reconciliation::Reconciliation> for ReconciliationSummary {
    fn from(r: reconciliation::Reconciliation) -> Self {
        ReconciliationSummary {
            total_starting_balance: r.total_starting_balance,
            total_ending_balance: r.total_ending_balance,
            net_balance_change: r.net_balance_change,
            net_cash_flow: r.net_cash_flow,
            implied_investment_gain: r.implied_investment_gain,
        }
    }
}

/// API view of a completed quarterly review: planned vs. actual figures, the
/// variance between them, the per-account balance detail, and the derived
/// reconciliation.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuarterlyReviewResponse {
    pub id: String,
    pub year: i32,
    pub quarter: i32,
    /// Human-friendly label, e.g. "2026 Q1".
    pub label: String,
    pub planned_income: f64,
    pub planned_spending: f64,
    pub planned_tax: f64,
    pub planned_withdrawal: f64,
    pub actual_income: f64,
    pub actual_spending: f64,
    pub actual_tax: f64,
    /// `actual_income - planned_income`.
    pub income_variance: f64,
    /// `actual_spending - planned_spending`.
    pub spending_variance: f64,
    /// `actual_tax - planned_tax`.
    pub tax_variance: f64,
    pub balances: Vec<ReviewAccountBalance>,
    pub reconciliation: ReconciliationSummary,
    pub notes: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
}

impl QuarterlyReviewResponse {
    /// Build a response from a persisted row, parsing its JSON-encoded
    /// account balances and deriving the reconciliation.
    pub fn from_row(row: &QuarterlyReview) -> AppResult<Self> {
        let balances: Vec<ReviewAccountBalance> = serde_json::from_str(&row.actual_balances)
            .map_err(|e| AppError::Internal(format!("corrupt quarterly review balances: {e}")))?;

        let changes: Vec<AccountBalanceChange> = balances
            .iter()
            .map(|b| AccountBalanceChange {
                starting_balance: b.starting_balance,
                ending_balance: b.ending_balance,
            })
            .collect();
        let reconciliation = reconciliation::reconcile(
            row.actual_income,
            row.actual_spending,
            row.actual_tax,
            &changes,
        );

        Ok(QuarterlyReviewResponse {
            id: row.id.clone(),
            year: row.year,
            quarter: row.quarter,
            label: format!("{} Q{}", row.year, row.quarter),
            planned_income: row.planned_income,
            planned_spending: row.planned_spending,
            planned_tax: row.planned_tax,
            planned_withdrawal: row.planned_withdrawal,
            actual_income: row.actual_income,
            actual_spending: row.actual_spending,
            actual_tax: row.actual_tax,
            income_variance: row.actual_income - row.planned_income,
            spending_variance: row.actual_spending - row.planned_spending,
            tax_variance: row.actual_tax - row.planned_tax,
            balances,
            reconciliation: reconciliation.into(),
            notes: row.notes.clone(),
            created_at: row.created_at,
        })
    }
}

/// An account's live balance, as shown for a quarter that still needs review.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DueAccountBalance {
    pub account_id: String,
    pub account_name: String,
    pub category: String,
    pub current_balance: f64,
}

/// A quarter that has not yet been reviewed, with the planned figures the
/// user will be comparing their actuals against.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DueQuarterlyReview {
    pub year: i32,
    pub quarter: i32,
    /// Human-friendly label, e.g. "2026 Q1".
    pub label: String,
    /// Whether this is the quarter currently in progress (as opposed to a
    /// past quarter that was simply never reviewed).
    pub is_current: bool,
    pub planned_income: f64,
    pub planned_spending: f64,
    pub planned_tax: f64,
    pub planned_withdrawal: f64,
    pub accounts: Vec<DueAccountBalance>,
}

/// Response for `GET /quarterly-reviews`: what still needs review, and the
/// history of what's already been completed.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuarterlyReviewOverview {
    pub due: Vec<DueQuarterlyReview>,
    pub history: Vec<QuarterlyReviewResponse>,
}
