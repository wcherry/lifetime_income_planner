//! Medicare IRMAA surcharge engine (roadmap Phase 3, feature 4).
//!
//! A pure, unit-tested engine that computes the Income-Related Monthly
//! Adjustment Amount (IRMAA) — the surcharge higher-income households pay on
//! top of the standard Medicare Part B and Part D premiums. Unlike the base
//! Part B premium (Phase 3, feature 3), which varies by plan choice, the
//! IRMAA surcharge amounts are fixed federal dollar figures set annually by
//! CMS, so both the Part B and Part D surcharges can be modeled directly
//! without the user having to supply a Part D plan premium.
//!
//! The mechanics:
//!
//! 1. **Two-year MAGI lookback.** IRMAA for a given year is based on the
//!    household's Modified Adjusted Gross Income from the tax return filed
//!    two years earlier (e.g. 2026 premiums use 2024 MAGI). The caller is
//!    responsible for supplying that lookback MAGI — this module only knows
//!    how to turn a MAGI figure into a surcharge.
//! 2. **Filing-status-specific brackets.** The IRS/CMS publish three bracket
//!    schedules: one shared by single, head-of-household, and qualifying
//!    widow(er) filers; one for joint filers (roughly double the single
//!    thresholds); and a narrower one for separate filers, since living with
//!    a spouse while filing separately can't use the full joint brackets.
//! 3. **Step tiers, not a curve.** Each bracket is a cliff: MAGI at or above a
//!    threshold pays that tier's flat monthly surcharge on both Part B and
//!    Part D, regardless of how far above the threshold it lands. Below the
//!    lowest threshold, there is no surcharge — the household pays only the
//!    standard premiums.
//!
//! Like the tax and ACA engines, the bracket thresholds and surcharge amounts
//! are **not hard-coded into the calculation**: they live in an [`IrmaaTables`]
//! value normally loaded from the database (admin-maintainable in a later
//! phase). [`IrmaaTables::default_2025`] holds the built-in 2025 figures used
//! to seed the database and to drive the unit tests. Thresholds are indexed to
//! the plan's general inflation rate from the table's base year so they keep
//! pace across a multi-decade projection — a simplification, since by statute
//! only the lower brackets are inflation-adjusted in reality and the top
//! bracket is frozen through 2027; this is a planning approximation, not tax
//! advice.

use crate::tax::FilingStatusKind;

/// Which of the three CMS bracket schedules a household's filing status maps
/// to. Single, head-of-household, and qualifying widow(er) share one
/// schedule; married-filing-jointly gets the widest brackets; married-filing-
/// separately gets its own narrow schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrmaaFilingGroup {
    Single,
    MarriedFilingJointly,
    MarriedFilingSeparately,
}

impl IrmaaFilingGroup {
    pub fn from_filing_status(fs: FilingStatusKind) -> Self {
        match fs {
            FilingStatusKind::MarriedFilingJointly | FilingStatusKind::QualifyingWidow => {
                IrmaaFilingGroup::MarriedFilingJointly
            }
            FilingStatusKind::MarriedFilingSeparately => IrmaaFilingGroup::MarriedFilingSeparately,
            FilingStatusKind::Single | FilingStatusKind::HeadOfHousehold => IrmaaFilingGroup::Single,
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "married_filing_jointly" => IrmaaFilingGroup::MarriedFilingJointly,
            "married_filing_separately" => IrmaaFilingGroup::MarriedFilingSeparately,
            _ => IrmaaFilingGroup::Single,
        }
    }
}

/// A single IRMAA bracket row as loaded from (or seeded into) the database:
/// at or above `magi_threshold`, both surcharges apply.
pub struct IrmaaBracketInput {
    pub filing_group: String,
    pub magi_threshold: f64,
    pub part_b_surcharge_monthly: f64,
    pub part_d_surcharge_monthly: f64,
}

/// One resolved bracket tier, sorted ascending by threshold within its group.
#[derive(Debug, Clone, Copy)]
struct Bracket {
    threshold: f64,
    part_b_monthly: f64,
    part_d_monthly: f64,
}

/// The reference parameters the IRMAA engine reads. Built from database rows
/// (see `crate::models::irmaa`) or from the built-in [`IrmaaTables::default_2025`].
#[derive(Debug, Clone)]
pub struct IrmaaTables {
    /// Year the loaded brackets were published for; the engine indexes
    /// forward from here by inflation.
    pub base_year: i32,
    single: Vec<Bracket>,
    married_filing_jointly: Vec<Bracket>,
    married_filing_separately: Vec<Bracket>,
}

impl IrmaaTables {
    /// Assemble tables from plain row inputs (as loaded from the database).
    pub fn from_inputs(base_year: i32, rows: Vec<IrmaaBracketInput>) -> Self {
        let mut single = Vec::new();
        let mut married_filing_jointly = Vec::new();
        let mut married_filing_separately = Vec::new();
        for r in rows {
            let bracket = Bracket {
                threshold: r.magi_threshold,
                part_b_monthly: r.part_b_surcharge_monthly,
                part_d_monthly: r.part_d_surcharge_monthly,
            };
            match IrmaaFilingGroup::from_str(&r.filing_group) {
                IrmaaFilingGroup::Single => single.push(bracket),
                IrmaaFilingGroup::MarriedFilingJointly => married_filing_jointly.push(bracket),
                IrmaaFilingGroup::MarriedFilingSeparately => married_filing_separately.push(bracket),
            }
        }
        let sort = |v: &mut Vec<Bracket>| {
            v.sort_by(|a, b| a.threshold.partial_cmp(&b.threshold).unwrap_or(std::cmp::Ordering::Equal))
        };
        sort(&mut single);
        sort(&mut married_filing_jointly);
        sort(&mut married_filing_separately);
        IrmaaTables {
            base_year,
            single,
            married_filing_jointly,
            married_filing_separately,
        }
    }

    /// The built-in 2025 CMS IRMAA brackets — the single source of truth used
    /// both to seed the database and to drive unit tests.
    pub fn default_2025() -> Self {
        let (year, rows) = default_2025_inputs();
        Self::from_inputs(year, rows)
    }

    fn brackets_for(&self, group: IrmaaFilingGroup) -> &[Bracket] {
        match group {
            IrmaaFilingGroup::Single => &self.single,
            IrmaaFilingGroup::MarriedFilingJointly => &self.married_filing_jointly,
            IrmaaFilingGroup::MarriedFilingSeparately => &self.married_filing_separately,
        }
    }
}

/// Inflation index factor applied to the bracket thresholds for a projection
/// year, relative to the table's base year.
fn inflation_factor(base_year: i32, year: i32, inflation_rate_pct: f64) -> f64 {
    let n = year - base_year;
    if n == 0 {
        1.0
    } else {
        (1.0 + inflation_rate_pct / 100.0).powi(n)
    }
}

/// Everything needed to compute one household's IRMAA tier for a year.
pub struct IrmaaInput {
    pub year: i32,
    /// General inflation rate (percent) used to index the brackets from the
    /// base year.
    pub inflation_rate: f64,
    /// The household's MAGI from two years prior — the actual CMS lookback.
    /// `None` when that history isn't available (see module docs), which is
    /// treated as no surcharge.
    pub lookback_magi: Option<f64>,
    pub filing_group: IrmaaFilingGroup,
}

/// The computed IRMAA tier detail for a year.
#[derive(Debug, Clone, Default)]
pub struct IrmaaResult {
    /// The MAGI the tier was determined from (0 when `lookback_magi` was `None`).
    pub lookback_magi: f64,
    /// Whether two-year-prior MAGI was available to determine the tier (vs.
    /// defaulting to no surcharge for lack of in-plan history).
    pub has_lookback_data: bool,
    /// This tier's Part B surcharge, per enrolled person, per month.
    pub part_b_surcharge_monthly: f64,
    /// This tier's Part D surcharge, per enrolled person, per month.
    pub part_d_surcharge_monthly: f64,
    /// Whether this tier carries any surcharge at all (MAGI at/above the
    /// lowest threshold).
    pub applies: bool,
}

/// Compute a household's IRMAA tier for one year using `tables`. Returns the
/// per-person monthly surcharge; the caller (the projection engine) applies
/// it once per Medicare-enrolled household member and annualizes it.
pub fn compute_irmaa(inp: &IrmaaInput, tables: &IrmaaTables) -> IrmaaResult {
    let Some(magi) = inp.lookback_magi else {
        return IrmaaResult::default();
    };
    let factor = inflation_factor(tables.base_year, inp.year, inp.inflation_rate);
    let brackets = tables.brackets_for(inp.filing_group);

    let mut tier: Option<Bracket> = None;
    for b in brackets {
        if magi >= b.threshold * factor {
            tier = Some(*b);
        } else {
            break; // sorted ascending, so nothing further can match either
        }
    }

    match tier {
        Some(b) => IrmaaResult {
            lookback_magi: magi,
            has_lookback_data: true,
            part_b_surcharge_monthly: b.part_b_monthly,
            part_d_surcharge_monthly: b.part_d_monthly,
            applies: true,
        },
        None => IrmaaResult {
            lookback_magi: magi,
            has_lookback_data: true,
            part_b_surcharge_monthly: 0.0,
            part_d_surcharge_monthly: 0.0,
            applies: false,
        },
    }
}

/// The built-in 2025 CMS IRMAA brackets as plain rows: `(base_year, rows)`.
/// Single source of truth used both to seed the `irmaa_brackets` table and to
/// build [`IrmaaTables::default_2025`]. Monthly surcharge amounts per the 2025
/// CMS Medicare Part B/Part D IRMAA tables.
pub fn default_2025_inputs() -> (i32, Vec<IrmaaBracketInput>) {
    let mut rows = Vec::new();

    // Single / head of household / qualifying widow(er).
    for (threshold, b, d) in [
        (106_000.0, 74.00, 13.70),
        (133_000.0, 185.00, 35.30),
        (167_000.0, 295.90, 57.00),
        (200_000.0, 406.90, 78.60),
        (500_000.0, 443.90, 85.60),
    ] {
        rows.push(IrmaaBracketInput {
            filing_group: "single".into(),
            magi_threshold: threshold,
            part_b_surcharge_monthly: b,
            part_d_surcharge_monthly: d,
        });
    }

    // Married filing jointly.
    for (threshold, b, d) in [
        (212_000.0, 74.00, 13.70),
        (266_000.0, 185.00, 35.30),
        (334_000.0, 295.90, 57.00),
        (400_000.0, 406.90, 78.60),
        (750_000.0, 443.90, 85.60),
    ] {
        rows.push(IrmaaBracketInput {
            filing_group: "married_filing_jointly".into(),
            magi_threshold: threshold,
            part_b_surcharge_monthly: b,
            part_d_surcharge_monthly: d,
        });
    }

    // Married filing separately (only two tiers — living with a spouse while
    // filing separately loses access to the wider joint brackets).
    for (threshold, b, d) in [(106_000.0, 406.90, 78.60), (394_000.0, 443.90, 85.60)] {
        rows.push(IrmaaBracketInput {
            filing_group: "married_filing_separately".into(),
            magi_threshold: threshold,
            part_b_surcharge_monthly: b,
            part_d_surcharge_monthly: d,
        });
    }

    (2025, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> IrmaaTables {
        IrmaaTables::default_2025()
    }

    fn input(magi: Option<f64>, group: IrmaaFilingGroup) -> IrmaaInput {
        IrmaaInput {
            year: 2025,
            inflation_rate: 0.0,
            lookback_magi: magi,
            filing_group: group,
        }
    }

    #[test]
    fn no_lookback_data_means_no_surcharge() {
        let out = compute_irmaa(&input(None, IrmaaFilingGroup::Single), &tables());
        assert!(!out.has_lookback_data);
        assert!(!out.applies);
        assert_eq!(out.part_b_surcharge_monthly, 0.0);
        assert_eq!(out.part_d_surcharge_monthly, 0.0);
    }

    #[test]
    fn below_the_lowest_threshold_has_no_surcharge() {
        let out = compute_irmaa(&input(Some(90_000.0), IrmaaFilingGroup::Single), &tables());
        assert!(out.has_lookback_data);
        assert!(!out.applies);
        assert_eq!(out.part_b_surcharge_monthly, 0.0);
    }

    #[test]
    fn first_single_tier_applies_at_the_threshold() {
        let out = compute_irmaa(&input(Some(106_000.0), IrmaaFilingGroup::Single), &tables());
        assert!(out.applies);
        assert_eq!(out.part_b_surcharge_monthly, 74.00);
        assert_eq!(out.part_d_surcharge_monthly, 13.70);
    }

    #[test]
    fn top_single_tier_applies_at_and_above_500k() {
        let at = compute_irmaa(&input(Some(500_000.0), IrmaaFilingGroup::Single), &tables());
        let above = compute_irmaa(&input(Some(2_000_000.0), IrmaaFilingGroup::Single), &tables());
        assert_eq!(at.part_b_surcharge_monthly, 443.90);
        assert_eq!(above.part_b_surcharge_monthly, 443.90);
        assert_eq!(above.part_d_surcharge_monthly, 85.60);
    }

    #[test]
    fn joint_thresholds_are_roughly_double_single() {
        // A couple's combined MAGI just below the joint second tier avoids the
        // surcharge that the same MAGI would trigger for a single filer.
        let single = compute_irmaa(
            &input(Some(150_000.0), IrmaaFilingGroup::Single),
            &tables(),
        );
        let joint = compute_irmaa(
            &input(Some(150_000.0), IrmaaFilingGroup::MarriedFilingJointly),
            &tables(),
        );
        assert!(single.part_b_surcharge_monthly > 0.0);
        assert_eq!(joint.part_b_surcharge_monthly, 0.0);
    }

    #[test]
    fn married_filing_separately_has_only_two_narrow_tiers() {
        let low = compute_irmaa(
            &input(Some(100_000.0), IrmaaFilingGroup::MarriedFilingSeparately),
            &tables(),
        );
        let mid = compute_irmaa(
            &input(Some(200_000.0), IrmaaFilingGroup::MarriedFilingSeparately),
            &tables(),
        );
        let high = compute_irmaa(
            &input(Some(500_000.0), IrmaaFilingGroup::MarriedFilingSeparately),
            &tables(),
        );
        assert!(!low.applies);
        assert_eq!(mid.part_b_surcharge_monthly, 406.90);
        assert_eq!(high.part_b_surcharge_monthly, 443.90);
    }

    #[test]
    fn thresholds_are_inflation_indexed() {
        // Twenty years out at 3% inflation, the same nominal MAGI that
        // triggered a surcharge today no longer reaches the (inflated) tier.
        let mut i = input(Some(110_000.0), IrmaaFilingGroup::Single);
        let now = compute_irmaa(&i, &tables());
        i.year = 2045;
        i.inflation_rate = 3.0;
        let later = compute_irmaa(&i, &tables());
        assert!(now.applies);
        assert!(!later.applies);
    }

    #[test]
    fn from_filing_status_groups_widow_with_joint_and_hoh_with_single() {
        assert_eq!(
            IrmaaFilingGroup::from_filing_status(FilingStatusKind::QualifyingWidow),
            IrmaaFilingGroup::MarriedFilingJointly
        );
        assert_eq!(
            IrmaaFilingGroup::from_filing_status(FilingStatusKind::HeadOfHousehold),
            IrmaaFilingGroup::Single
        );
    }
}
