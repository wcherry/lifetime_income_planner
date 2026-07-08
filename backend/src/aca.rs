//! ACA premium tax credit engine (roadmap Phase 3, feature 1).
//!
//! A pure, unit-tested engine that computes a household's Affordable Care Act
//! marketplace subsidy (the premium tax credit) for a single year. The credit
//! caps what a household pays for the benchmark plan (the second-lowest-cost
//! silver plan, "SLCSP") at an income-based "applicable percentage" of its
//! Modified Adjusted Gross Income (MAGI); the government pays the rest.
//!
//! The mechanics:
//!
//! 1. **MAGI vs. the Federal Poverty Line.** Household MAGI is measured as a
//!    percentage of the Federal Poverty Line (FPL) for the household size. Below
//!    100% of FPL there is no premium tax credit (that population falls to
//!    Medicaid), so the subsidy is zero.
//! 2. **Applicable percentage.** Where the household lands on the FPL scale sets
//!    the share of income it is expected to contribute toward the benchmark
//!    premium — a piecewise-linear curve running from 0% at/below 150% FPL up to
//!    8.5% at 400%+ FPL (the enhanced schedule in effect through 2025, with no
//!    "subsidy cliff").
//! 3. **The credit.** Expected contribution = applicable percentage × MAGI. The
//!    premium tax credit is the benchmark premium minus that contribution,
//!    floored at zero.
//!
//! Like the tax engine, the FPL guidelines and the applicable-percentage curve
//! are **not hard-coded into the calculation**: they live in an [`AcaTables`]
//! value normally loaded from the database (admin-maintainable in a later
//! phase). [`AcaTables::default_2025`] holds the built-in 2025 figures used to
//! seed the database and to drive the unit tests. FPL guidelines are indexed to
//! the plan's general inflation rate from the table's base year so they keep
//! pace across a multi-decade projection.
//!
//! This is a planning approximation, not enrollment advice: it models the
//! contiguous-48-states FPL, a single benchmark premium supplied by the user,
//! and does not model Medicaid-expansion gaps between 100% and 138% FPL, the
//! family-glitch rules, or employer-coverage disqualification.

use std::collections::HashMap;

/// A Federal Poverty Line row as loaded from (or seeded into) the database.
pub struct FplInput {
    pub household_size: i32,
    pub annual_amount: f64,
}

/// An applicable-percentage breakpoint: at `fpl_percent` of the poverty line the
/// household is expected to contribute `applicable_percentage` (a fraction) of
/// MAGI. Values between breakpoints are linearly interpolated.
pub struct ApplicablePercentageInput {
    pub fpl_percent: f64,
    pub applicable_percentage: f64,
}

/// The reference parameters the ACA engine reads. Built from database rows (see
/// `crate::models::aca`) or from the built-in [`AcaTables::default_2025`].
#[derive(Debug, Clone)]
pub struct AcaTables {
    /// Year the loaded FPL guidelines were published for; the engine indexes
    /// forward from here by inflation.
    pub base_year: i32,
    /// Base-year FPL annual dollar amount by household size.
    fpl: HashMap<i32, f64>,
    /// Applicable-percentage curve, sorted ascending by `fpl_percent`.
    applicable: Vec<(f64, f64)>,
}

impl AcaTables {
    /// Assemble tables from plain row inputs (as loaded from the database).
    pub fn from_inputs(
        base_year: i32,
        fpl_rows: Vec<FplInput>,
        applicable_rows: Vec<ApplicablePercentageInput>,
    ) -> Self {
        let mut fpl: HashMap<i32, f64> = HashMap::new();
        for r in fpl_rows {
            fpl.insert(r.household_size, r.annual_amount);
        }
        let mut applicable: Vec<(f64, f64)> = applicable_rows
            .into_iter()
            .map(|r| (r.fpl_percent, r.applicable_percentage))
            .collect();
        applicable.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        AcaTables {
            base_year,
            fpl,
            applicable,
        }
    }

    /// The built-in 2025 ACA parameters — the single source of truth used both
    /// to seed the tables and to drive unit tests.
    pub fn default_2025() -> Self {
        let (year, fpl, applicable) = default_2025_inputs();
        Self::from_inputs(year, fpl, applicable)
    }

    /// Base-year Federal Poverty Line for a household size, falling back to the
    /// largest tabulated size when the requested size is beyond the table.
    fn base_fpl(&self, household_size: i32) -> f64 {
        if let Some(v) = self.fpl.get(&household_size) {
            return *v;
        }
        // Fall back to the largest size at or below the request, else the
        // smallest available (keeps the engine robust to sparse tables).
        self.fpl
            .iter()
            .filter(|(k, _)| **k <= household_size)
            .max_by_key(|(k, _)| **k)
            .or_else(|| self.fpl.iter().min_by_key(|(k, _)| **k))
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }

    /// The applicable percentage (fraction of MAGI expected as contribution) at
    /// a given percentage of the Federal Poverty Line, linearly interpolated
    /// between breakpoints and clamped to the ends of the curve.
    fn applicable_percentage(&self, fpl_percent: f64) -> f64 {
        let curve = &self.applicable;
        match curve.first() {
            None => 0.0,
            Some(&(first_x, first_y)) => {
                if fpl_percent <= first_x {
                    return first_y;
                }
                for w in curve.windows(2) {
                    let (x0, y0) = w[0];
                    let (x1, y1) = w[1];
                    if fpl_percent <= x1 {
                        let t = if x1 > x0 { (fpl_percent - x0) / (x1 - x0) } else { 0.0 };
                        return y0 + t * (y1 - y0);
                    }
                }
                curve.last().map(|&(_, y)| y).unwrap_or(0.0)
            }
        }
    }
}

/// Inflation index factor applied to the FPL guidelines for a projection year,
/// relative to the table's base year.
fn inflation_factor(base_year: i32, year: i32, inflation_rate_pct: f64) -> f64 {
    let n = year - base_year;
    if n == 0 {
        1.0
    } else {
        (1.0 + inflation_rate_pct / 100.0).powi(n)
    }
}

/// Everything needed to compute one household's ACA subsidy for a year.
pub struct AcaInput {
    pub year: i32,
    /// General inflation rate (percent) used to index the FPL from the base year.
    pub inflation_rate: f64,
    /// Household Modified Adjusted Gross Income (annual dollars).
    pub magi: f64,
    /// Tax-household size (1 or 2 for a retirement couple; larger with dependents).
    pub household_size: i32,
    /// Annual benchmark (second-lowest silver) premium the household faces.
    pub benchmark_annual_premium: f64,
}

/// The computed ACA subsidy detail for a year.
#[derive(Debug, Clone, Default)]
pub struct AcaResult {
    pub magi: f64,
    /// Federal Poverty Line for the household size this year (inflation-indexed).
    pub federal_poverty_line: f64,
    /// MAGI as a percentage of the poverty line (e.g. 250.0 for 250%).
    pub fpl_percent: f64,
    /// Share of MAGI the household is expected to contribute (fraction).
    pub applicable_percentage: f64,
    /// Annual dollars the household is expected to pay toward the benchmark.
    pub expected_contribution: f64,
    /// The benchmark premium used (annual dollars).
    pub benchmark_premium: f64,
    /// The premium tax credit (annual subsidy dollars).
    pub subsidy: f64,
    /// Whether the household qualifies for a premium tax credit this year.
    pub eligible: bool,
}

/// Compute a household's ACA premium tax credit for one year using `tables`.
pub fn compute_subsidy(inp: &AcaInput, tables: &AcaTables) -> AcaResult {
    let factor = inflation_factor(tables.base_year, inp.year, inp.inflation_rate);
    let fpl = tables.base_fpl(inp.household_size.max(1)) * factor;
    let magi = inp.magi.max(0.0);
    let fpl_percent = if fpl > 0.0 { magi / fpl * 100.0 } else { 0.0 };

    // A premium tax credit requires a benchmark plan and income at or above the
    // poverty line; below 100% FPL the household falls to Medicaid, not the
    // marketplace subsidy.
    let eligible = inp.benchmark_annual_premium > 0.0 && fpl > 0.0 && fpl_percent >= 100.0;

    let applicable_percentage = tables.applicable_percentage(fpl_percent);
    let expected_contribution = applicable_percentage * magi;
    let subsidy = if eligible {
        (inp.benchmark_annual_premium - expected_contribution).max(0.0)
    } else {
        0.0
    };

    AcaResult {
        magi,
        federal_poverty_line: fpl,
        fpl_percent,
        applicable_percentage,
        expected_contribution,
        benchmark_premium: inp.benchmark_annual_premium,
        subsidy,
        eligible,
    }
}

/// The built-in 2025 ACA parameters as plain rows: `(base_year, FPL guidelines,
/// applicable-percentage curve)`. Single source of truth used both to seed the
/// `aca_*` tables and to build [`AcaTables::default_2025`].
pub fn default_2025_inputs() -> (i32, Vec<FplInput>, Vec<ApplicablePercentageInput>) {
    // 2025 HHS Federal Poverty Guidelines, 48 contiguous states + DC: $15,650
    // for a household of one, plus $5,500 for each additional person.
    let base = 15_650.0;
    let increment = 5_500.0;
    let fpl: Vec<FplInput> = (1..=8)
        .map(|n| FplInput {
            household_size: n,
            annual_amount: base + increment * (n as f64 - 1.0),
        })
        .collect();

    // Applicable-percentage curve (the enhanced schedule in effect through 2025
    // under the Inflation Reduction Act): 0% at/below 150% FPL, rising linearly
    // to 8.5% at 400% FPL and flat at 8.5% above it — no subsidy cliff.
    let applicable = vec![
        (150.0, 0.0),
        (200.0, 0.02),
        (250.0, 0.04),
        (300.0, 0.06),
        (400.0, 0.085),
    ]
    .into_iter()
    .map(|(fpl_percent, applicable_percentage)| ApplicablePercentageInput {
        fpl_percent,
        applicable_percentage,
    })
    .collect();

    (2025, fpl, applicable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> AcaTables {
        AcaTables::default_2025()
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1.0, "expected ~{b}, got {a}");
    }

    fn input(magi: f64, household_size: i32, benchmark: f64) -> AcaInput {
        AcaInput {
            year: 2025,
            inflation_rate: 0.0,
            magi,
            household_size,
            benchmark_annual_premium: benchmark,
        }
    }

    #[test]
    fn below_poverty_line_gets_no_credit() {
        // Single filer at ~90% FPL (magi 14,000 of 15,650): Medicaid territory,
        // no premium tax credit.
        let out = compute_subsidy(&input(14_000.0, 1, 12_000.0), &tables());
        assert!(!out.eligible);
        assert_eq!(out.subsidy, 0.0);
        assert!(out.fpl_percent < 100.0);
    }

    #[test]
    fn between_100_and_150_fpl_pays_nothing_and_is_fully_subsidized() {
        // Single filer at ~125% FPL: applicable percentage is 0, so the entire
        // benchmark premium is covered.
        let magi = 1.25 * 15_650.0;
        let out = compute_subsidy(&input(magi, 1, 9_000.0), &tables());
        assert!(out.eligible);
        approx(out.fpl_percent, 125.0);
        assert_eq!(out.applicable_percentage, 0.0);
        approx(out.expected_contribution, 0.0);
        approx(out.subsidy, 9_000.0);
    }

    #[test]
    fn applicable_percentage_interpolates_between_breakpoints() {
        // Single filer at 225% FPL sits halfway between the 200% (2%) and 250%
        // (4%) breakpoints, so the applicable percentage is 3%.
        let magi = 2.25 * 15_650.0;
        let out = compute_subsidy(&input(magi, 1, 12_000.0), &tables());
        approx(out.fpl_percent, 225.0);
        assert!((out.applicable_percentage - 0.03).abs() < 1e-6);
        approx(out.expected_contribution, 0.03 * magi);
        approx(out.subsidy, 12_000.0 - 0.03 * magi);
    }

    #[test]
    fn no_subsidy_cliff_above_400_fpl_caps_contribution_at_8_5_percent() {
        // A couple well above 400% FPL still qualifies (cliff removed): expected
        // contribution is capped at 8.5% of MAGI.
        let magi = 90_000.0; // ~426% of the 2-person FPL (21,150)
        let out = compute_subsidy(&input(magi, 2, 20_000.0), &tables());
        assert!(out.eligible);
        assert!(out.fpl_percent > 400.0);
        assert!((out.applicable_percentage - 0.085).abs() < 1e-6);
        approx(out.expected_contribution, 0.085 * magi);
        approx(out.subsidy, 20_000.0 - 0.085 * magi);
    }

    #[test]
    fn subsidy_is_zero_when_contribution_exceeds_benchmark() {
        // High income, cheap benchmark: expected contribution outstrips the
        // premium, so there is no credit (but no negative subsidy either).
        let out = compute_subsidy(&input(200_000.0, 2, 8_000.0), &tables());
        assert_eq!(out.subsidy, 0.0);
    }

    #[test]
    fn no_benchmark_premium_means_no_subsidy() {
        let out = compute_subsidy(&input(40_000.0, 2, 0.0), &tables());
        assert!(!out.eligible);
        assert_eq!(out.subsidy, 0.0);
    }

    #[test]
    fn household_size_changes_the_poverty_line() {
        // The same MAGI is a smaller share of FPL for a larger household, so a
        // couple gets a bigger subsidy than a single filer.
        let single = compute_subsidy(&input(40_000.0, 1, 15_000.0), &tables());
        let couple = compute_subsidy(&input(40_000.0, 2, 15_000.0), &tables());
        assert!(couple.fpl_percent < single.fpl_percent);
        assert!(couple.subsidy > single.subsidy);
    }

    #[test]
    fn poverty_line_is_inflation_indexed() {
        // Twenty years out at 3% inflation the FPL is much higher, so the same
        // nominal MAGI is a smaller share of it.
        let mut i = input(40_000.0, 1, 15_000.0);
        let now = compute_subsidy(&i, &tables());
        i.year = 2045;
        i.inflation_rate = 3.0;
        let later = compute_subsidy(&i, &tables());
        assert!(later.federal_poverty_line > now.federal_poverty_line);
        assert!(later.fpl_percent < now.fpl_percent);
    }
}
