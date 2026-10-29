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
/// ACA subsidy modeling is off by default (a benchmark premium of 0 disables it).
pub const DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM: f64 = 0.0;
/// Medicare Part B modeling (Phase 3, feature 3) defaults to *on*, using the
/// 2025 standard monthly premium ($185.00 x 12) — nearly every retiree pays at
/// least this once enrolled, so it is a realistic default rather than an
/// opt-in like the ACA benchmark. 0 disables it.
pub const DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM: f64 = 2_220.0;
/// "Don't incur penalties" (withdrawal sequencing) defaults to *off*: the
/// engine may draw from tax-deferred accounts before age 59½ (paying the 10%
/// early-withdrawal penalty) when that's what a spending need requires.
pub const DEFAULT_AVOID_EARLY_WITHDRAWAL_PENALTY: bool = false;
/// Medicare enrollment age (Phase 3, feature 3) defaults to 65 — the
/// standard Initial Enrollment Period age — unless the user models delaying
/// enrollment (e.g. other creditable coverage).
pub const DEFAULT_MEDICARE_ENROLLMENT_AGE: i32 = 65;
/// Fixed-ceiling Roth conversion sizing is the default; the IRMAA/RMD-aware
/// mode (feature 6) is opt-in.
pub const DEFAULT_ROTH_CONVERSION_MODE: &str = "fixed_ceiling";

/// Roth conversion sizing mode (roadmap Phase 3, feature 6): how much to
/// convert from tax-deferred to Roth each year.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RothConversionMode {
    /// Fill taxable income to a flat, user-chosen AGI ceiling
    /// (`roth_conversion_ceiling`) every year.
    #[default]
    FixedCeiling,
    /// Size each year's conversion to stay just under the household's next
    /// Medicare IRMAA bracket two years out (the actual CMS MAGI lookback),
    /// tapering off as RMDs fill that room. `roth_conversion_ceiling`, if
    /// also set, acts as an extra upper bound rather than the sole driver.
    IrmaaAware,
}

impl RothConversionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RothConversionMode::FixedCeiling => "fixed_ceiling",
            RothConversionMode::IrmaaAware => "irmaa_aware",
        }
    }

    /// Parse the stored string form, defaulting to fixed-ceiling for anything
    /// unrecognized.
    pub fn from_str(s: &str) -> Self {
        match s {
            "irmaa_aware" => RothConversionMode::IrmaaAware,
            _ => RothConversionMode::FixedCeiling,
        }
    }
}

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
    /// Runs the same tax-aware sequencing as `TaxOptimized`, plus a search
    /// (feature 9b) over several levels of voluntary tax-deferred withdrawals
    /// that "fill" the current tax bracket each year — shrinking tax-deferred
    /// balances before RMDs force larger, possibly higher-bracket withdrawals
    /// later. The engine runs multiple full projections internally and keeps
    /// whichever level produces the greatest total lifetime after-tax cash.
    MaximizeIncome,
}

impl WithdrawalStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            WithdrawalStrategy::Conventional => "conventional",
            WithdrawalStrategy::TaxOptimized => "tax_optimized",
            WithdrawalStrategy::MaximizeIncome => "maximize_income",
        }
    }

    /// Parse the stored string form, defaulting to conventional for anything
    /// unrecognized.
    pub fn from_str(s: &str) -> Self {
        match s {
            "tax_optimized" => WithdrawalStrategy::TaxOptimized,
            "maximize_income" => WithdrawalStrategy::MaximizeIncome,
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
    /// ACA subsidy modeling (Phase 3, feature 1): the annual benchmark (SLCSP)
    /// premium the household faces. 0 disables ACA subsidy modeling.
    pub aca_benchmark_annual_premium: f64,
    /// Withdrawal sequencing strategy (feature 9), stored as its `as_str()` form.
    pub withdrawal_strategy: String,
    /// Medicare Part B modeling (Phase 3, feature 3): the annual standard Part
    /// B premium per Medicare-enrolled household member. 0 disables it.
    pub medicare_part_b_annual_premium: f64,
    /// "Don't incur penalties" (withdrawal sequencing): when true, tax-deferred
    /// accounts are excluded from the drawdown order for an owner who hasn't
    /// reached the age-59½ "Penalty-free withdrawals" milestone, rather than
    /// drawing from them and paying the 10% early-withdrawal penalty.
    pub avoid_early_withdrawal_penalty: bool,
    /// Medicare enrollment age (Phase 3, feature 3): the age at which Part B
    /// premiums and IRMAA enrollment counting begin for each household
    /// member. Defaults to 65; a later age models delayed enrollment.
    pub medicare_enrollment_age: i32,
    /// Roth conversion sizing mode (feature 6), stored as its `as_str()` form.
    pub roth_conversion_mode: String,
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
    pub aca_benchmark_annual_premium: f64,
    pub withdrawal_strategy: String,
    pub medicare_part_b_annual_premium: f64,
    pub avoid_early_withdrawal_penalty: bool,
    pub medicare_enrollment_age: i32,
    pub roth_conversion_mode: String,
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

    /// ACA benchmark (SLCSP) annual premium (Phase 3, feature 1). 0 disables
    /// ACA subsidy modeling.
    #[validate(range(min = 0.0, max = 1_000_000.0, message = "must be between 0 and 1,000,000"))]
    #[serde(default)]
    #[schema(example = 0.0)]
    pub aca_benchmark_annual_premium: f64,

    /// Withdrawal sequencing strategy (feature 9). Defaults to conventional.
    #[serde(default)]
    pub withdrawal_strategy: WithdrawalStrategy,

    /// Medicare Part B annual premium (Phase 3, feature 3): the standard
    /// premium per Medicare-enrolled household member, applied automatically
    /// from age 65. 0 disables it.
    #[validate(range(min = 0.0, max = 50_000.0, message = "must be between 0 and 50,000"))]
    #[serde(default = "default_medicare_part_b_annual_premium")]
    #[schema(example = 2_220.0)]
    pub medicare_part_b_annual_premium: f64,

    /// "Don't incur penalties" (withdrawal sequencing). When true, the engine
    /// never draws from a tax-deferred account before its owner turns 59½,
    /// even if that leaves a spending shortfall. Defaults to off.
    #[serde(default)]
    #[schema(example = false)]
    pub avoid_early_withdrawal_penalty: bool,

    /// Medicare enrollment age (Phase 3, feature 3): the age at which Part B
    /// premiums and IRMAA enrollment counting begin, per household member.
    /// Defaults to 65; a later age (up to 70) models delaying enrollment
    /// because of other creditable coverage.
    #[validate(range(min = 65, max = 70, message = "must be between 65 and 70"))]
    #[serde(default = "default_medicare_enrollment_age")]
    #[schema(example = 65)]
    pub medicare_enrollment_age: i32,

    /// Roth conversion sizing mode (feature 6). Defaults to a flat ceiling.
    #[serde(default)]
    pub roth_conversion_mode: RothConversionMode,
}

fn default_medicare_part_b_annual_premium() -> f64 {
    DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM
}

fn default_medicare_enrollment_age() -> i32 {
    DEFAULT_MEDICARE_ENROLLMENT_AGE
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
    pub aca_benchmark_annual_premium: f64,
    pub withdrawal_strategy: WithdrawalStrategy,
    pub medicare_part_b_annual_premium: f64,
    pub avoid_early_withdrawal_penalty: bool,
    pub medicare_enrollment_age: i32,
    pub roth_conversion_mode: RothConversionMode,
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
            aca_benchmark_annual_premium: DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM,
            withdrawal_strategy: WithdrawalStrategy::Conventional,
            medicare_part_b_annual_premium: DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM,
            avoid_early_withdrawal_penalty: DEFAULT_AVOID_EARLY_WITHDRAWAL_PENALTY,
            medicare_enrollment_age: DEFAULT_MEDICARE_ENROLLMENT_AGE,
            roth_conversion_mode: RothConversionMode::FixedCeiling,
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
            aca_benchmark_annual_premium: a.aca_benchmark_annual_premium,
            withdrawal_strategy: WithdrawalStrategy::from_str(&a.withdrawal_strategy),
            medicare_part_b_annual_premium: a.medicare_part_b_annual_premium,
            avoid_early_withdrawal_penalty: a.avoid_early_withdrawal_penalty,
            medicare_enrollment_age: a.medicare_enrollment_age,
            roth_conversion_mode: RothConversionMode::from_str(&a.roth_conversion_mode),
            is_default: false,
            updated_at: Some(a.updated_at),
        }
    }
}
