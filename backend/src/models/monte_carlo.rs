use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

/// Default annual-return volatility (standard deviation, percentage points)
/// used when a Monte Carlo request omits it.
pub const DEFAULT_MONTE_CARLO_VOLATILITY: f64 = 12.0;

/// Request body to run a Monte Carlo simulation (roadmap Phase 4, feature 6).
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct MonteCarloRequest {
    /// How many independent trials to run. The roadmap calls out 1,000 /
    /// 5,000 / 10,000 as the standard presets; the range allows any value in
    /// between for flexibility.
    #[validate(range(min = 100, max = 20_000, message = "must be between 100 and 20,000"))]
    #[schema(example = 1000)]
    pub num_simulations: u32,

    /// Standard deviation of each year's investment-return shock, in
    /// percentage points (e.g. `12.0` = 12%), applied identically to every
    /// account's `expected_roi` each simulated year (a single market-wide
    /// shock per year, modeling systematic market risk). Defaults to 12.0.
    #[validate(range(min = 0.0, max = 60.0, message = "must be between 0 and 60"))]
    #[serde(default = "default_volatility")]
    #[schema(example = 12.0)]
    pub volatility: f64,
}

fn default_volatility() -> f64 {
    DEFAULT_MONTE_CARLO_VOLATILITY
}

/// One projection year's ending-balance percentile band across all simulations.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MonteCarloYearBand {
    pub year: i32,
    pub p10: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p90: f64,
}

/// Aggregate Monte Carlo simulation result (roadmap Phase 4, feature 6).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MonteCarloResult {
    pub num_simulations: u32,
    pub volatility: f64,
    /// Fraction (0.0-1.0) of simulations in which the plan's money lasted the
    /// entire horizon without a shortfall.
    pub success_rate: f64,
    pub median_ending_balance: f64,
    pub best_case_ending_balance: f64,
    pub worst_case_ending_balance: f64,
    /// Per-year ending-balance percentile bands (p10/p25/p50/p75/p90) across
    /// all simulations — a "fan chart" the frontend renders directly.
    pub percentile_bands: Vec<MonteCarloYearBand>,
}
