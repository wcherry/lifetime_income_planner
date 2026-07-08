use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::assumptions;

/// Default planning assumptions, used when a user has not saved their own yet.
pub const DEFAULT_INFLATION_RATE: f64 = 2.5;
pub const DEFAULT_INVESTMENT_RETURN_RATE: f64 = 6.0;
pub const DEFAULT_HEALTHCARE_INFLATION_RATE: f64 = 4.5;
pub const DEFAULT_SOCIAL_SECURITY_COLA_RATE: f64 = 2.0;
/// Roth conversions are off by default (a ceiling of 0 disables them).
pub const DEFAULT_ROTH_CONVERSION_CEILING: f64 = 0.0;
/// The conventional (taxable -> tax-deferred -> tax-free) sequencing is the
/// default; tax-optimized sequencing (feature 9) is opt-in.
pub const DEFAULT_WITHDRAWAL_STRATEGY: &str = "conventional";

/// Withdrawal sequencing strategy (roadmap Phase 2, feature 9): which order the
/// engine draws from accounts to cover a year's cash need.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalStrategy {
    /// Taxable accounts fully before tax-deferred, then tax-free — the
    /// standard rule of thumb that preserves tax-advantaged growth longest.
    #[default]
    Conventional,
    /// Reorders taxable accounts by ascending embedded gain (realizing the
    /// cheapest gains first) and, in years where realizing a gain would cost
    /// more at the margin than an equivalent ordinary withdrawal, draws
    /// tax-deferred funds before taxable ones.
    TaxOptimized,
}

impl WithdrawalStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            WithdrawalStrategy::Conventional => "conventional",
            WithdrawalStrategy::TaxOptimized => "tax_optimized",
        }
    }

    /// Parse the stored string form, defaulting to conventional for anything
    /// unrecognized.
    pub fn from_str(s: &str) -> Self {
        match s {
            "tax_optimized" => WithdrawalStrategy::TaxOptimized,
            _ => WithdrawalStrategy::Conventional,
        }
    }
}

/// Persisted assumptions row (one per user).
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = assumptions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Assumptions {
    pub id: String,
    pub user_id: String,
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    /// Roth conversion strategy (feature 6): convert traditional -> Roth each
    /// year until taxable income reaches this ceiling. 0 disables conversions.
    pub roth_conversion_ceiling: f64,
    /// Optional first/last calendar years the conversion strategy applies.
    pub roth_conversion_start_year: Option<i32>,
    pub roth_conversion_end_year: Option<i32>,
    /// Withdrawal sequencing strategy (feature 9), stored as its `as_str()` form.
    pub withdrawal_strategy: String,
}

/// Insertable row used when assumptions are first created.
#[derive(Insertable)]
#[diesel(table_name = assumptions)]
pub struct NewAssumptions {
    pub id: String,
    pub user_id: String,
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub updated_at: NaiveDateTime,
    pub roth_conversion_ceiling: f64,
    pub roth_conversion_start_year: Option<i32>,
    pub roth_conversion_end_year: Option<i32>,
    pub withdrawal_strategy: String,
}

/// Request body for creating or replacing the planning assumptions.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AssumptionsRequest {
    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 2.5)]
    pub inflation_rate: f64,

    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 6.0)]
    pub investment_return_rate: f64,

    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 4.5)]
    pub healthcare_inflation_rate: f64,

    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 2.0)]
    pub social_security_cola_rate: f64,

    /// Roth conversion ceiling (feature 6): fill taxable income up to this many
    /// dollars each year by converting traditional -> Roth. 0 disables it.
    #[validate(range(min = 0.0, max = 100_000_000.0, message = "must be between 0 and 100,000,000"))]
    #[serde(default)]
    #[schema(example = 0.0)]
    pub roth_conversion_ceiling: f64,

    /// Optional window during which conversions run. When omitted, conversions
    /// apply from the start of (or through the end of) the projection.
    #[validate(range(min = 1900, max = 2200, message = "must be a valid year"))]
    #[serde(default)]
    pub roth_conversion_start_year: Option<i32>,

    #[validate(range(min = 1900, max = 2200, message = "must be a valid year"))]
    #[serde(default)]
    pub roth_conversion_end_year: Option<i32>,

    /// Withdrawal sequencing strategy (feature 9). Defaults to conventional.
    #[serde(default)]
    pub withdrawal_strategy: WithdrawalStrategy,
}

/// API view of the planning assumptions.
#[derive(Serialize, ToSchema)]
pub struct AssumptionsResponse {
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub roth_conversion_ceiling: f64,
    pub roth_conversion_start_year: Option<i32>,
    pub roth_conversion_end_year: Option<i32>,
    pub withdrawal_strategy: WithdrawalStrategy,
    /// True when no assumptions have been saved yet and defaults are being returned.
    pub is_default: bool,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub updated_at: Option<NaiveDateTime>,
}

impl AssumptionsResponse {
    /// The response returned before a user has saved any assumptions of their own.
    pub fn defaults() -> Self {
        AssumptionsResponse {
            inflation_rate: DEFAULT_INFLATION_RATE,
            investment_return_rate: DEFAULT_INVESTMENT_RETURN_RATE,
            healthcare_inflation_rate: DEFAULT_HEALTHCARE_INFLATION_RATE,
            social_security_cola_rate: DEFAULT_SOCIAL_SECURITY_COLA_RATE,
            roth_conversion_ceiling: DEFAULT_ROTH_CONVERSION_CEILING,
            roth_conversion_start_year: None,
            roth_conversion_end_year: None,
            withdrawal_strategy: WithdrawalStrategy::Conventional,
            is_default: true,
            updated_at: None,
        }
    }
}

impl From<Assumptions> for AssumptionsResponse {
    fn from(a: Assumptions) -> Self {
        AssumptionsResponse {
            inflation_rate: a.inflation_rate,
            investment_return_rate: a.investment_return_rate,
            healthcare_inflation_rate: a.healthcare_inflation_rate,
            social_security_cola_rate: a.social_security_cola_rate,
            roth_conversion_ceiling: a.roth_conversion_ceiling,
            roth_conversion_start_year: a.roth_conversion_start_year,
            roth_conversion_end_year: a.roth_conversion_end_year,
            withdrawal_strategy: WithdrawalStrategy::from_str(&a.withdrawal_strategy),
            is_default: false,
            updated_at: Some(a.updated_at),
        }
    }
}
