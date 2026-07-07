use serde::Serialize;
use utoipa::ToSchema;

/// The planning assumptions actually used to build a projection, echoed back so
/// the client can show what drove the numbers.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectionAssumptions {
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    /// True when the user has not saved assumptions and defaults were used.
    pub is_default: bool,
}

/// Headline figures summarizing the whole projection.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectionSummary {
    /// Sum of all account balances today, before any growth.
    pub current_net_worth: f64,
    /// Total balance across all accounts at the end of the plan (the estate).
    pub projected_ending_balance: f64,
    pub total_lifetime_income: f64,
    pub total_lifetime_spending: f64,
    pub total_lifetime_withdrawals: f64,
    /// First year in which spending could not be fully funded, if any.
    pub depletion_year: Option<i32>,
}

/// A single life event occurring within a projection year.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LifeEventOccurrence {
    pub name: String,
    /// Signed cash flow for this year: positive for inflows, negative for outflows.
    pub amount: f64,
}

/// An age- or date-based planning milestone (not a cash flow), e.g. Medicare
/// eligibility or the start of required minimum distributions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Milestone {
    /// Short name for the marker, e.g. "Medicare eligibility".
    pub label: String,
    /// One-line explanation for the tooltip.
    pub detail: String,
    /// Age of the relevant person when the milestone is reached.
    pub age: i32,
}

/// One calendar year of the projection.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct YearProjection {
    pub year: i32,
    pub primary_age: i32,
    pub spouse_age: Option<i32>,
    /// Total account balance at the start of the year.
    pub starting_balance: f64,
    pub income: f64,
    pub spending: f64,
    /// Net signed cash flow from life events (inflows positive).
    pub life_events_net: f64,
    /// The individual life events that occur this year (for markers/tooltips).
    pub life_events: Vec<LifeEventOccurrence>,
    /// Age/regulatory milestones reached this year (for markers/tooltips).
    pub milestones: Vec<Milestone>,
    /// Investment growth credited to accounts this year.
    pub growth: f64,
    /// Total drawn from accounts to cover the shortfall.
    pub withdrawals: f64,
    /// Surplus cash reinvested into accounts.
    pub contributions: f64,
    /// Total account balance at the end of the year.
    pub ending_balance: f64,
    /// Spending that could not be funded because accounts were exhausted.
    pub shortfall: f64,
}

/// A single account's withdrawal within a quarter of the near-term schedule.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuarterWithdrawal {
    pub account_id: String,
    pub account_name: String,
    pub category: String,
    pub amount: f64,
}

/// One quarter of the actionable near-term withdrawal schedule.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuarterProjection {
    /// Human-friendly label, e.g. "2026 Q1".
    pub label: String,
    pub year: i32,
    pub quarter: i32,
    pub income: f64,
    pub spending: f64,
    pub total_withdrawal: f64,
    pub withdrawals: Vec<QuarterWithdrawal>,
}

/// Full projection response: the annual engine output (feature 8) plus the
/// near-term quarterly withdrawal schedule (feature 9).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectionResponse {
    pub current_year: i32,
    pub start_year: i32,
    pub end_year: i32,
    pub assumptions: ProjectionAssumptions,
    pub summary: ProjectionSummary,
    pub annual: Vec<YearProjection>,
    pub quarterly: Vec<QuarterProjection>,
}
