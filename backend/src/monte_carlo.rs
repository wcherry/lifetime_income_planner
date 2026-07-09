//! Monte Carlo simulation (roadmap Phase 4, feature 6): re-runs the pure
//! projection engine many times with randomized annual investment-return
//! shocks to report a probability of plan success and percentile outcome
//! bands, rather than the single deterministic path `run_projection`
//! produces on its own.

use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

use crate::models::{MonteCarloResult, MonteCarloYearBand};
use crate::projection::{run_projection_with_shocks, ProjectionInputs};

/// Run `num_simulations` independent trials, each perturbing every account's
/// expected return by a per-year `Normal(0, volatility)` shock (one shock per
/// year, shared across all accounts that year — a market-wide shock, not an
/// independent one per account). Returns the aggregate success rate and
/// per-year ending-balance percentile bands.
pub fn run_monte_carlo(
    inputs: &ProjectionInputs,
    num_simulations: u32,
    volatility: f64,
) -> MonteCarloResult {
    // A shockless run establishes the plan horizon (which years exist) without
    // duplicating `run_projection`'s end-year computation here.
    let baseline = run_projection_with_shocks(inputs, &[]);
    let years: Vec<i32> = baseline.annual.iter().map(|y| y.year).collect();
    let horizon = years.len().max(1);

    let mut rng = rand::rngs::StdRng::from_entropy();
    let normal = Normal::new(0.0, volatility.max(0.0))
        .expect("volatility must be a finite, non-negative standard deviation");

    let n = num_simulations.max(1) as usize;
    let mut final_balances: Vec<f64> = Vec::with_capacity(n);
    let mut successes: u32 = 0;
    let mut year_balances: Vec<Vec<f64>> = vec![Vec::with_capacity(n); horizon];

    for _ in 0..n {
        let shocks: Vec<f64> = (0..horizon).map(|_| normal.sample(&mut rng)).collect();
        let result = run_projection_with_shocks(inputs, &shocks);

        if result.summary.depletion_year.is_none() {
            successes += 1;
        }
        final_balances.push(result.summary.projected_ending_balance);
        for (i, y) in result.annual.iter().enumerate().take(horizon) {
            year_balances[i].push(y.ending_balance);
        }
    }

    final_balances.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let percentile_bands = years
        .into_iter()
        .zip(year_balances)
        .map(|(year, mut balances)| {
            balances.sort_by(|a, b| a.partial_cmp(b).unwrap());
            MonteCarloYearBand {
                year,
                p10: percentile(&balances, 10.0),
                p25: percentile(&balances, 25.0),
                p50: percentile(&balances, 50.0),
                p75: percentile(&balances, 75.0),
                p90: percentile(&balances, 90.0),
            }
        })
        .collect();

    MonteCarloResult {
        num_simulations,
        volatility,
        success_rate: successes as f64 / n as f64,
        median_ending_balance: percentile(&final_balances, 50.0),
        best_case_ending_balance: final_balances.last().copied().unwrap_or(0.0),
        worst_case_ending_balance: final_balances.first().copied().unwrap_or(0.0),
        percentile_bands,
    }
}

/// Linear-interpolated percentile of an already-sorted slice (`p` in `0..=100`).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0) * (sorted.len() as f64 - 1.0);
    let lo = idx.floor() as usize;
    let hi = (idx.ceil() as usize).min(sorted.len() - 1);
    if lo == hi {
        sorted[lo]
    } else {
        let frac = idx - lo as f64;
        sorted[lo] + (sorted[hi] - sorted[lo]) * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    use crate::aca::AcaTables;
    use crate::irmaa::IrmaaTables;
    use crate::models::{Account, Profile};
    use crate::tax::TaxTables;

    fn ts() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn profile(dob_year: i32, life_expectancy: i32) -> Profile {
        Profile {
            id: "p1".into(),
            user_id: "u1".into(),
            first_name: "Jane".into(),
            last_name: "Doe".into(),
            date_of_birth: date(dob_year, 6, 1),
            marital_status: "single".into(),
            filing_status: "single".into(),
            state: "TX".into(),
            retirement_date: date(2026, 1, 1),
            life_expectancy,
            spouse_first_name: None,
            spouse_last_name: None,
            spouse_date_of_birth: None,
            spouse_life_expectancy: None,
            created_at: ts(),
            updated_at: ts(),
        }
    }

    fn account(id: &str, category: &str, balance: f64, roi: f64) -> Account {
        Account {
            id: id.into(),
            user_id: "u1".into(),
            name: format!("acct-{id}"),
            category: category.into(),
            account_type: "brokerage".into(),
            owner: "self".into(),
            current_balance: balance,
            expected_roi: roi,
            dividend_yield: 0.0,
            cost_basis: None,
            allocation_stock_pct: None,
            allocation_bond_pct: None,
            allocation_cash_pct: None,
            withdrawal_restrictions: None,
            created_at: ts(),
            updated_at: ts(),
        }
    }

    fn base_inputs<'a>(profile: &'a Profile, accounts: &'a [Account]) -> ProjectionInputs<'a> {
        ProjectionInputs {
            current_year: 2026,
            profile,
            accounts,
            income: &[],
            spending: &[],
            life_events: &[],
            inflation_rate: 0.0,
            investment_return_rate: 0.0,
            healthcare_inflation_rate: 0.0,
            social_security_cola_rate: 0.0,
            assumptions_are_default: false,
            roth_conversion_ceiling: 0.0,
            roth_conversion_start_year: None,
            roth_conversion_end_year: None,
            withdrawal_strategy: "conventional".to_string(),
            aca_benchmark_annual_premium: 0.0,
            medicare_part_b_annual_premium: 0.0,
            tax_tables: TaxTables::default_2025(),
            aca_tables: AcaTables::default_2025(),
            irmaa_tables: IrmaaTables::default_2025(),
        }
    }

    #[test]
    fn zero_volatility_has_no_variance_across_the_percentile_band() {
        let p = profile(1960, 90);
        let accts = [account("a1", "taxable", 500_000.0, 5.0)];
        let inputs = base_inputs(&p, &accts);

        let deterministic = run_projection_with_shocks(&inputs, &[]);
        let expected_success = deterministic.summary.depletion_year.is_none();

        let result = run_monte_carlo(&inputs, 100, 0.0);

        assert_eq!(
            result.success_rate,
            if expected_success { 1.0 } else { 0.0 }
        );
        for band in &result.percentile_bands {
            assert!((band.p10 - band.p90).abs() < 1e-6);
            assert!((band.p10 - band.p50).abs() < 1e-6);
        }
    }

    #[test]
    fn percentile_bands_are_monotonic_per_year() {
        let p = profile(1960, 90);
        let accts = [account("a1", "taxable", 500_000.0, 5.0)];
        let inputs = base_inputs(&p, &accts);

        let result = run_monte_carlo(&inputs, 200, 15.0);

        for band in &result.percentile_bands {
            assert!(band.p10 <= band.p25);
            assert!(band.p25 <= band.p50);
            assert!(band.p50 <= band.p75);
            assert!(band.p75 <= band.p90);
        }
    }

    #[test]
    fn runs_the_requested_number_of_simulations_without_panicking() {
        let p = profile(1960, 90);
        let accts = [account("a1", "taxable", 500_000.0, 5.0)];
        let inputs = base_inputs(&p, &accts);

        let deterministic = run_projection_with_shocks(&inputs, &[]);
        let result = run_monte_carlo(&inputs, 100, 12.0);

        assert_eq!(result.num_simulations, 100);
        assert_eq!(result.percentile_bands.len(), deterministic.annual.len());
    }
}
