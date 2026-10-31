//! Projection engine (roadmap Phase 1, features 8 & 9; Phase 2, features 1–5).
//!
//! Given a user's profile, accounts, income, spending, life events, and
//! planning assumptions, this produces a year-by-year cash-flow projection and
//! a near-term quarterly withdrawal schedule. It is now *tax-aware*: each year
//! it categorizes income (ordinary, qualified dividends, capital gains, Social
//! Security), computes federal and state tax via [`crate::tax`], and funds that
//! tax from account withdrawals alongside spending. Because withdrawing from a
//! tax-deferred account itself creates taxable income, the withdrawal amount
//! and the tax are solved together by a short fixed-point iteration each year.
//! The engine is pure (no I/O) so it can be unit tested in isolation.

use std::collections::HashMap;

use chrono::{Datelike, Months, NaiveDate};

use crate::aca::{compute_subsidy, AcaInput, AcaResult, AcaTables};
use crate::irmaa::{compute_irmaa, IrmaaFilingGroup, IrmaaInput, IrmaaTables};
use crate::models::{
    Account, EstimatedTaxPayment, EstimatedTaxes, IncomeSource, LifeEvent, LifeEventOccurrence,
    Milestone, Profile, ProjectionAssumptions, ProjectionResponse, ProjectionSummary,
    QuarterProjection, QuarterWithdrawal, SpendingItem, YearAca, YearAccountBalance, YearIrmaa,
    YearProjection, YearTax,
};
use crate::tax::{compute_taxes, FilingStatusKind, TaxInput, TaxResult, TaxTables};

/// IRS additional tax (IRC §72(t)) on tax-deferred withdrawals taken before
/// the owner reaches the "Penalty-free withdrawals" milestone (age 59½).
const EARLY_WITHDRAWAL_PENALTY_RATE: f64 = 0.10;

/// Everything the engine needs to build a projection. Rates are percentages
/// (e.g. `2.5` means 2.5%).
pub struct ProjectionInputs<'a> {
    pub current_year: i32,
    pub profile: &'a Profile,
    pub accounts: &'a [Account],
    pub income: &'a [IncomeSource],
    pub spending: &'a [SpendingItem],
    pub life_events: &'a [LifeEvent],
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub assumptions_are_default: bool,
    /// Roth conversion strategy (feature 6): convert traditional -> Roth each
    /// year until taxable income reaches this ceiling (in dollars). 0 disables.
    pub roth_conversion_ceiling: f64,
    /// Optional first/last years the conversion strategy runs.
    pub roth_conversion_start_year: Option<i32>,
    pub roth_conversion_end_year: Option<i32>,
    /// Withdrawal sequencing strategy (Phase 2, feature 9): `"conventional"`
    /// (default) or `"tax_optimized"`. Unrecognized values fall back to
    /// conventional.
    pub withdrawal_strategy: String,
    /// ACA benchmark (SLCSP) annual premium (Phase 3, feature 1). 0 disables ACA
    /// subsidy modeling. Grows with the healthcare inflation rate over the plan.
    pub aca_benchmark_annual_premium: f64,
    /// Medicare Part B annual premium per enrolled household member (Phase 3,
    /// feature 3), as of `current_year`. 0 disables Medicare cost modeling.
    /// Applied automatically once each person turns 65 and grows with the
    /// healthcare inflation rate over the plan.
    pub medicare_part_b_annual_premium: f64,
    /// "Don't incur penalties" (withdrawal sequencing): when true, tax-deferred
    /// accounts are excluded from the drawdown order entirely for an owner who
    /// hasn't reached the "Penalty-free withdrawals" milestone (age 59½),
    /// rather than drawing from them and paying the 10% early-withdrawal
    /// penalty. Roth conversions are unaffected — they aren't withdrawals.
    pub avoid_early_withdrawal_penalty: bool,
    /// Medicare enrollment age (Phase 3, feature 3): the age at which Part B
    /// premiums and IRMAA enrollment counting begin, per household member.
    /// Defaults to 65; a later age models delayed enrollment.
    pub medicare_enrollment_age: i32,
    /// Roth conversion sizing mode (Phase 3, feature 6): `"fixed_ceiling"`
    /// (default; a flat AGI ceiling every year) or `"irmaa_aware"` (size the
    /// conversion to stay under the next Medicare IRMAA bracket two years
    /// out, tapering as RMDs fill that room).
    pub roth_conversion_mode: String,
    /// Reference tax parameters (brackets, deductions, state rates), loaded from
    /// the database and read by the tax engine.
    pub tax_tables: TaxTables,
    /// Reference ACA parameters (FPL guidelines, applicable-percentage curve).
    pub aca_tables: AcaTables,
    /// Reference Medicare IRMAA parameters (bracket thresholds and surcharges
    /// by filing status, Phase 3 feature 4).
    pub irmaa_tables: IrmaaTables,
}

/// Withdrawal sequencing strategy (Phase 2, feature 9), parsed from the stored
/// assumptions string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WithdrawalStrategyKind {
    /// Taxable accounts fully before tax-deferred, then tax-free.
    Conventional,
    /// Reorders taxable accounts by ascending embedded gain and, in years
    /// where realizing a gain costs more at the margin than an equivalent
    /// ordinary withdrawal, draws tax-deferred funds before taxable ones.
    TaxOptimized,
    /// Tax-optimized sequencing plus a search over bracket-filling
    /// acceleration levels (feature 9b) — see [`run_projection_inner`].
    MaximizeIncome,
}

impl WithdrawalStrategyKind {
    fn from_str(s: &str) -> Self {
        match s {
            "tax_optimized" => WithdrawalStrategyKind::TaxOptimized,
            "maximize_income" => WithdrawalStrategyKind::MaximizeIncome,
            _ => WithdrawalStrategyKind::Conventional,
        }
    }
}

/// Roth conversion sizing mode (Phase 3, feature 6), parsed from the stored
/// assumptions string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RothConversionModeKind {
    /// Fill taxable income to a flat, user-chosen AGI ceiling every year.
    FixedCeiling,
    /// Size the conversion to stay under the next Medicare IRMAA bracket two
    /// years out, tapering as RMDs fill that room.
    IrmaaAware,
    /// Convert as much as keeps MAGI at or under the ACA subsidy cutoff.
    AcaAware,
}

impl RothConversionModeKind {
    fn from_str(s: &str) -> Self {
        match s {
            "irmaa_aware" => RothConversionModeKind::IrmaaAware,
            "aca_aware" => RothConversionModeKind::AcaAware,
            _ => RothConversionModeKind::FixedCeiling,
        }
    }
}

/// A buffer (dollars) below the ACA cutoff so MAGI lands strictly under it.
const ACA_AWARE_CONVERSION_BUFFER: f64 = 1.0;

/// Most times a year is re-solved while searching for the largest conversion
/// that keeps MAGI under the ACA cutoff (one more pass then settles on the
/// best conversion found).
const ACA_AWARE_MAX_ATTEMPTS: usize = 8;

/// A conversion this close under the cutoff, in dollars, is close enough.
const ACA_AWARE_TOLERANCE: f64 = 1.0;

/// Bracket-fill targets searched by the maximize-income strategy (feature
/// 9b): each `Some(n)` means "voluntarily draw tax-deferred dollars every
/// year to fill ordinary bracket `n` and below" (`n = 0` is the lowest
/// bracket), before RMDs would otherwise force the issue, possibly at a
/// higher rate. `None` is the do-nothing candidate — the same behavior as
/// `TaxOptimized`. Each candidate is run as a full, independent projection;
/// the one with the greatest total lifetime after-tax cash wins.
const MAXIMIZE_INCOME_BRACKET_TARGETS: [Option<usize>; 5] =
    [None, Some(0), Some(1), Some(2), Some(3)];

/// The objective the maximize-income strategy (feature 9b) searches for.
/// Spending, income, and Medicare/IRMAA costs are plan inputs that don't vary
/// between bracket-fill candidates, and every candidate fully funds the same
/// spending plan unless accounts run dry — so, holding consumption fixed, the
/// household's total lifetime income is maximized by whichever candidate:
///
/// 1. Never triggers a depletion that a less aggressive candidate would have
///    avoided (running out of money is strictly worse than any paper estate
///    difference), then
/// 2. Leaves the largest after-tax ending balance — i.e. lost the least of
///    the household's resources to lifetime tax along the way.
fn maximize_income_score(summary: &ProjectionSummary) -> f64 {
    if summary.depletion_year.is_some() {
        f64::MIN
    } else {
        summary.projected_ending_balance
    }
}

/// Withdrawal priority by tax category. Taxable money is spent first, then
/// tax-deferred, then tax-free (preserving tax-free growth for last). Accounts
/// in the "other" bucket (pensions, cash-value life) are not liquid drawdown
/// accounts and are excluded from the pool.
fn category_priority(category: &str) -> Option<u8> {
    match category {
        "taxable" => Some(0),
        "tax_deferred" => Some(1),
        "tax_free" => Some(2),
        _ => None, // "other" — not drawn down
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Annualize a raw amount given a frequency string.
fn annualize(amount: f64, frequency: &str) -> f64 {
    match frequency {
        "monthly" => amount * 12.0,
        _ => amount,
    }
}

/// Compound-growth factor `(1 + rate/100)^years`, clamped so a negative number
/// of years never inflates a figure.
fn growth_factor(rate_pct: f64, years: i32) -> f64 {
    if years <= 0 {
        1.0
    } else {
        (1.0 + rate_pct / 100.0).powi(years)
    }
}

/// Income received in a given calendar year from a single source.
fn income_for_year(src: &IncomeSource, year: i32, ss_cola: f64) -> f64 {
    let start = src.start_date.year();
    let ends_after = src.end_date.map_or(true, |d| d.year() >= year);
    if start > year || !ends_after {
        return 0.0;
    }
    let base = annualize(src.amount, &src.frequency);
    // Explicit growth plus, when the source carries a COLA, the assumed
    // cost-of-living rate. Both are rarely set at once.
    let mut rate = src.growth_rate;
    if src.cola {
        rate += ss_cola;
    }
    base * growth_factor(rate, year - start)
            * active_fraction_of_year(src.start_date, src.end_date, year)

}

/// Share of a calendar year that falls within `start..=end` (both inclusive,
/// counted by day within each month), so income that starts or stops mid-year
/// is prorated instead of counted for the whole year.
fn active_fraction_of_year(start: NaiveDate, end: Option<NaiveDate>, year: i32) -> f64 {
    let days_in_month = |y: i32, m: u32| -> f64 {
        let first = NaiveDate::from_ymd_opt(y, m, 1).expect("valid month");
        let next = first + Months::new(1);
        next.signed_duration_since(first).num_days() as f64
    };
    // Months elapsed before the source begins.
    let mut first = 0.0;
    if start.year() == year {
        first = (start.month() - 1) as f64
            + (start.day() - 1) as f64 / days_in_month(year, start.month());
    }
    // Months elapsed through the day the source ends.
    let mut last = 12.0;
    if let Some(end) = end {
        if end.year() == year {
            last = (end.month() - 1) as f64 + end.day() as f64 / days_in_month(year, end.month());
        }
    }
    (last.min(12.0) - first).max(0.0) / 12.0
}

/// Spending incurred in a given calendar year for a single item.
fn spending_for_year(
    item: &SpendingItem,
    year: i32,
    current_year: i32,
    inflation: f64,
    healthcare_inflation: f64,
) -> f64 {
    let active = if item.frequency == "one_time" {
        // A one-time expense lands in its start year, defaulting to now.
        year == item.start_year.unwrap_or(current_year)
    } else {
        item.start_year.map_or(true, |s| s <= year)
            && item.end_year.map_or(true, |e| e >= year)
    };
    if !active {
        return 0.0;
    }
    let base = annualize(item.amount, &item.frequency);
    if !item.inflation_adjusted {
        return base;
    }
    // Healthcare costs track healthcare inflation; everything else general.
    let rate = if item.category == "healthcare" {
        healthcare_inflation
    } else {
        inflation
    };
    base * growth_factor(rate, year - current_year)
}

/// Signed cash flow from a single life event in a given calendar year
/// (inflows positive, outflows negative).
fn life_event_for_year(event: &LifeEvent, year: i32, current_year: i32, inflation: f64) -> f64 {
    let occurs = match event.recurrence.as_str() {
        "one_time" => year == event.event_date.year(),
        _ => {
            event.event_date.year() <= year
                && event.end_date.map_or(true, |d| d.year() >= year)
        }
    };
    if !occurs {
        return 0.0;
    }
    let base = annualize(event.amount, &event.recurrence);
    let inflated = if event.inflation_adjusted {
        base * growth_factor(inflation, year - current_year)
    } else {
        base
    };
    let signed = if event.direction == "outflow" {
        -inflated
    } else {
        inflated
    };
    signed
}

/// Calendar year in which `dob` reaches the given age in whole months.
fn milestone_year(dob: NaiveDate, months: u32) -> Option<i32> {
    dob.checked_add_months(Months::new(months)).map(|d| d.year())
}

/// Social Security full retirement age, in months, by birth year (SSA schedule).
fn full_retirement_age_months(birth_year: i32) -> u32 {
    match birth_year {
        y if y <= 1954 => 66 * 12,
        1955 => 66 * 12 + 2,
        1956 => 66 * 12 + 4,
        1957 => 66 * 12 + 6,
        1958 => 66 * 12 + 8,
        1959 => 66 * 12 + 10,
        _ => 67 * 12,
    }
}

/// Age at which required minimum distributions must begin (SECURE 2.0).
fn rmd_age(birth_year: i32) -> i32 {
    if birth_year <= 1950 {
        72
    } else if birth_year <= 1959 {
        73
    } else {
        75
    }
}

/// IRS Uniform Lifetime Table (Pub. 590-B) distribution period by age, used
/// to compute required minimum distributions. `None` below the table's
/// first entry (RMDs never apply that young); ages past the last entry use
/// its divisor.
fn rmd_divisor(age: i32) -> Option<f64> {
    const TABLE: &[f64] = &[
        27.4, 26.5, 25.5, 24.6, 23.7, 22.9, 22.0, 21.1, 20.2, 19.4, // 72-81
        18.5, 17.7, 16.8, 16.0, 15.2, 14.4, 13.7, 12.9, 12.2, 11.5, // 82-91
        10.8, 10.1, 9.5, 8.9, 8.4, 7.8, 7.3, 6.8, 6.4, 6.0, // 92-101
        5.6, 5.2, 4.9, 4.6, 4.3, 4.1, 3.9, 3.7, 3.5, 3.4, // 102-111
        3.3, 3.1, 3.0, 2.9, 2.8, 2.7, 2.5, 2.3, 2.0, // 112-120
    ];
    const FIRST_AGE: i32 = 72;
    if age < FIRST_AGE {
        return None;
    }
    let idx = ((age - FIRST_AGE) as usize).min(TABLE.len() - 1);
    Some(TABLE[idx])
}

/// Sum of tax-deferred account balances belonging to one owner bucket, at a
/// given balance snapshot. Retirement accounts can't legally be jointly
/// titled, so "joint"-tagged tax-deferred accounts are attributed to the
/// plan's primary owner alongside "self" accounts.
fn tax_deferred_balance_for_owner(accounts: &[Account], balances: &[f64], spouse: bool) -> f64 {
    (0..accounts.len())
        .filter(|&i| accounts[i].category == "tax_deferred")
        .filter(|&i| (accounts[i].owner == "spouse") == spouse)
        .map(|i| balances[i])
        .sum()
}

/// Household RMD due for the year (RMD module): each owner's tax-deferred
/// balance at the *start* of the year (the prior year-end balance, before
/// this year's growth) divided by their IRS Uniform Lifetime Table divisor,
/// once they've reached their own RMD age. `spouse` is `(birth_year, age)`.
fn compute_household_rmd(
    accounts: &[Account],
    starting_balances: &[f64],
    primary_birth_year: i32,
    primary_age: i32,
    spouse: Option<(i32, i32)>,
) -> f64 {
    let mut total = 0.0;
    if primary_age >= rmd_age(primary_birth_year) {
        if let Some(div) = rmd_divisor(primary_age) {
            let bal = tax_deferred_balance_for_owner(accounts, starting_balances, false);
            if bal > 0.0 {
                total += bal / div;
            }
        }
    }
    if let Some((spouse_birth_year, spouse_age)) = spouse {
        if spouse_age >= rmd_age(spouse_birth_year) {
            if let Some(div) = rmd_divisor(spouse_age) {
                let bal = tax_deferred_balance_for_owner(accounts, starting_balances, true);
                if bal > 0.0 {
                    total += bal / div;
                }
            }
        }
    }
    total
}

/// Household Medicare Part B premiums due for the year (Phase 3, feature 3):
/// the standard annual premium, inflation-indexed from `current_year` by the
/// healthcare inflation rate, charged once per household member who has
/// reached `enrollment_age` this year (Phase 3, feature 3: defaults to 65,
/// but a later age models someone delaying enrollment for other creditable
/// coverage). `spouse_birth_year` is the spouse's birth year, if any. This
/// models the base premium only — the IRMAA income-based surcharge is
/// computed separately.
fn compute_medicare_part_b_premiums(
    annual_premium: f64,
    primary_birth_year: i32,
    year: i32,
    current_year: i32,
    healthcare_inflation_rate: f64,
    spouse_birth_year: Option<i32>,
    enrollment_age: i32,
) -> f64 {
    if annual_premium <= 0.0 {
        return 0.0;
    }
    let per_person =
        annual_premium * growth_factor(healthcare_inflation_rate, year - current_year);
    let mut total = 0.0;
    if year - primary_birth_year >= enrollment_age {
        total += per_person;
    }
    if let Some(sby) = spouse_birth_year {
        if year - sby >= enrollment_age {
            total += per_person;
        }
    }
    total
}

/// Modified Adjusted Gross Income (Phase 3, feature 2): AGI plus the untaxed
/// portion of Social Security benefits. Computed the same way regardless of
/// whether ACA eligibility applies, so it can be tracked and forecast across
/// every year of the plan (and, in a later phase, drive Medicare IRMAA).
fn compute_magi(tax: &TaxResult, ss_benefits: f64) -> f64 {
    tax.adjusted_gross_income + (ss_benefits - tax.taxable_social_security).max(0.0)
}

/// Age/regulatory milestones for one person. `who` is a lowercase subject
/// phrase ("you" / "your spouse") woven into each tooltip.
fn person_milestones(dob: NaiveDate, who: &str) -> Vec<(i32, Milestone)> {
    let by = dob.year();
    let mut out: Vec<(i32, Milestone)> = Vec::new();

    if let Some(y) = milestone_year(dob, 59 * 12 + 6) {
        out.push((
            y,
            Milestone {
                label: "Penalty-free withdrawals".into(),
                detail: format!("Penalty-free IRA and 401(k) withdrawals for {who} (age 59½)."),
                age: 59,
                is_regulatory: false,
            },
        ));
    }
    out.push((
        by + 62,
        Milestone {
            label: "Social Security eligibility".into(),
            detail: format!("Earliest Social Security for {who} (age 62, reduced benefit)."),
            age: 62,
            is_regulatory: false,
        },
    ));
    // Medicare enrollment window (Phase 3, feature 3): the Initial Enrollment
    // Period runs from 3 months before the 65th birthday month through 3
    // months after it (7 months total). Enrolling after it closes risks a
    // lifetime Part B late-enrollment penalty (absent other creditable
    // coverage), so both edges of the window are surfaced alongside the
    // eligibility date itself.
    if let Some(y) = milestone_year(dob, 64 * 12 + 9) {
        out.push((
            y,
            Milestone {
                label: "Medicare enrollment window opens".into(),
                detail: format!(
                    "The Medicare Initial Enrollment Period begins for {who} — 3 months before turning 65."
                ),
                age: 64,
                is_regulatory: true,
            },
        ));
    }
    out.push((
        by + 65,
        Milestone {
            label: "Medicare eligibility".into(),
            detail: format!("Medicare eligibility begins for {who} (age 65)."),
            age: 65,
            is_regulatory: false,
        },
    ));
    if let Some(y) = milestone_year(dob, 65 * 12 + 3) {
        out.push((
            y,
            Milestone {
                label: "Medicare enrollment window closes".into(),
                detail: format!(
                    "The Medicare Initial Enrollment Period ends for {who} — enrolling after this \
                     without other creditable coverage risks a lifetime Part B late-enrollment penalty."
                ),
                age: 65,
                is_regulatory: true,
            },
        ));
    }
    let fra_m = full_retirement_age_months(by);
    if let Some(y) = milestone_year(dob, fra_m) {
        let (yrs, rem) = (fra_m / 12, fra_m % 12);
        let fra = if rem == 0 {
            format!("{yrs}")
        } else {
            format!("{yrs} yrs {rem} mos")
        };
        out.push((
            y,
            Milestone {
                label: "Full retirement age".into(),
                detail: format!("Full Social Security retirement age for {who} ({fra})."),
                age: yrs as i32,
                is_regulatory: false,
            },
        ));
    }
    out.push((
        by + 70,
        Milestone {
            label: "Maximum Social Security".into(),
            detail: format!("Delayed retirement credits stop growing for {who} (age 70)."),
            age: 70,
            is_regulatory: false,
        },
    ));
    let ra = rmd_age(by);
    out.push((
        by + ra,
        Milestone {
            label: "RMDs begin".into(),
            detail: format!("Required minimum distributions must begin for {who} (age {ra})."),
            age: ra,
            is_regulatory: true,
        },
    ));
    out
}

/// Build a per-year map of all milestones within the projection horizon.
fn build_milestones(profile: &Profile, start_year: i32, end_year: i32) -> HashMap<i32, Vec<Milestone>> {
    let mut all = person_milestones(profile.date_of_birth, "you");
    if let Some(sdob) = profile.spouse_date_of_birth {
        all.extend(person_milestones(sdob, "your spouse"));
    }
    // The planned retirement date is a milestone in its own right.
    all.push((
        profile.retirement_date.year(),
        Milestone {
            label: "Planned retirement".into(),
            detail: "Your planned retirement date.".into(),
            age: profile.retirement_date.year() - profile.date_of_birth.year(),
            is_regulatory: false,
        },
    ));

    let mut map: HashMap<i32, Vec<Milestone>> = HashMap::new();
    for (year, m) in all {
        if year >= start_year && year <= end_year {
            map.entry(year).or_default().push(m);
        }
    }
    map
}

/// Run the projection. Always returns at least the current year.
pub fn run_projection(inp: &ProjectionInputs) -> ProjectionResponse {
    run_projection_dispatch(inp, None)
}

/// Run the projection with a Monte Carlo per-year investment-return shock
/// (roadmap Phase 4, feature 6) layered on top of every account's
/// `expected_roi`. `return_shocks[i]` (percentage points, e.g. `-8.0` for an
/// 8-point-worse year) perturbs the year at offset `i` from `inp.current_year`;
/// a shocks slice shorter than the plan horizon is fine — years past its end
/// are unperturbed.
pub fn run_projection_with_shocks(
    inp: &ProjectionInputs,
    return_shocks: &[f64],
) -> ProjectionResponse {
    run_projection_dispatch(inp, Some(return_shocks))
}

/// Dispatches to a single projection pass, except for the maximize-income
/// strategy (feature 9b), which instead runs one pass per candidate
/// acceleration level and keeps whichever produced the greatest total
/// lifetime after-tax cash — the "runs simulations" step that lets the
/// strategy see the future RMD/tax consequence of today's withdrawal, rather
/// than only reacting to the RMD once it comes due.
fn run_projection_dispatch(
    inp: &ProjectionInputs,
    return_shocks: Option<&[f64]>,
) -> ProjectionResponse {
    if WithdrawalStrategyKind::from_str(&inp.withdrawal_strategy)
        != WithdrawalStrategyKind::MaximizeIncome
    {
        return run_projection_inner(inp, return_shocks, None);
    }

    MAXIMIZE_INCOME_BRACKET_TARGETS
        .into_iter()
        .map(|target| run_projection_inner(inp, return_shocks, target))
        .max_by(|a, b| {
            maximize_income_score(&a.summary)
                .partial_cmp(&maximize_income_score(&b.summary))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("MAXIMIZE_INCOME_BRACKET_TARGETS is non-empty")
}

/// `bracket_fill_target` (feature 9b, maximize-income strategy only): when
/// `Some(n)`, voluntarily draws tax-deferred dollars every year (beyond
/// whatever spending and RMDs already require) to fill ordinary bracket `n`
/// and below. Ignored for every other strategy.
fn run_projection_inner(
    inp: &ProjectionInputs,
    return_shocks: Option<&[f64]>,
    bracket_fill_target: Option<usize>,
) -> ProjectionResponse {
    let start_year = inp.current_year;
    let birth_year = inp.profile.date_of_birth.year();
    let mut end_year = birth_year + inp.profile.life_expectancy;
    // Married plans run until the last survivor's life expectancy.
    if let (Some(sdob), Some(sle)) = (
        inp.profile.spouse_date_of_birth,
        inp.profile.spouse_life_expectancy,
    ) {
        end_year = end_year.max(sdob.year() + sle);
    }
    // Always project at least the current year.
    if end_year < start_year {
        end_year = start_year;
    }

    let spouse_birth_year = inp.profile.spouse_date_of_birth.map(|d| d.year());

    // Same 59½ threshold as the "Penalty-free withdrawals" milestone below —
    // computed once here so it can actually be priced into tax-deferred
    // withdrawals, not just marked on the timeline.
    let primary_penalty_free_year = milestone_year(inp.profile.date_of_birth, 59 * 12 + 6);
    let spouse_penalty_free_year = inp
        .profile
        .spouse_date_of_birth
        .and_then(|d| milestone_year(d, 59 * 12 + 6));

    let mut milestones = build_milestones(inp.profile, start_year, end_year);

    // Running per-account balances, parallel to `inp.accounts`.
    let mut balances: Vec<f64> = inp.accounts.iter().map(|a| a.current_balance).collect();
    // Running per-account cost basis, used to realize capital gains on taxable
    // withdrawals. When a taxable account has no stated basis we assume basis
    // equals its balance (no embedded gain), so users must supply a cost basis
    // to model capital gains. Non-taxable accounts ignore this.
    let mut basis: Vec<f64> = inp
        .accounts
        .iter()
        .map(|a| {
            if a.category == "taxable" {
                a.cost_basis.unwrap_or(a.current_balance)
            } else {
                0.0
            }
        })
        .collect();

    let filing_status = FilingStatusKind::from_str(&inp.profile.filing_status);
    let withdrawal_strategy = WithdrawalStrategyKind::from_str(&inp.withdrawal_strategy);
    let roth_mode = RothConversionModeKind::from_str(&inp.roth_conversion_mode);
    // ACA tax-household size: a joint return is a two-person household, everyone
    // else is modeled as one (dependents are not tracked in this phase).
    let household_size = match filing_status {
        FilingStatusKind::MarriedFilingJointly | FilingStatusKind::QualifyingWidow => 2,
        _ => 1,
    };

    // Drawdown order: category priority, then largest balance first, then
    // original order for stability. "other" accounts are excluded.
    let mut order: Vec<usize> = (0..inp.accounts.len())
        .filter(|&i| category_priority(&inp.accounts[i].category).is_some())
        .collect();
    order.sort_by(|&a, &b| {
        let pa = category_priority(&inp.accounts[a].category).unwrap();
        let pb = category_priority(&inp.accounts[b].category).unwrap();
        pa.cmp(&pb)
            .then(
                inp.accounts[b]
                    .current_balance
                    .partial_cmp(&inp.accounts[a].current_balance)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.cmp(&b))
    });
    // Roth conversions (feature 6) land in the first tax-free account. Without
    // one there is nowhere to hold the converted dollars, so conversions are
    // skipped even if a ceiling is configured.
    let roth_dest = order
        .iter()
        .copied()
        .find(|&i| inp.accounts[i].category == "tax_free");
    // Surplus is reinvested into the first drawdown account (taxable first,
    // then whatever's next in `order`). The maximize-income strategy (feature
    // 9b) instead prefers taxable, then tax-free, over tax-deferred: its
    // voluntary bracket-fill withdrawals must never land back in a
    // tax-deferred account, since that would tax the same dollar today for no
    // benefit (undoing the whole point of pulling it out early) rather than
    // permanently moving it out of future RMDs' reach.
    let taxable_dest = order
        .iter()
        .copied()
        .find(|&i| inp.accounts[i].category == "taxable");
    let reinvest_target = if withdrawal_strategy == WithdrawalStrategyKind::MaximizeIncome {
        taxable_dest.or(roth_dest).or(order.first().copied())
    } else {
        order.first().copied()
    };

    let current_net_worth: f64 = balances.iter().sum();

    let mut annual: Vec<YearProjection> = Vec::new();
    let mut first_year_withdrawals: Vec<QuarterWithdrawal> = Vec::new();
    let mut first_year_tax = 0.0;
    let mut total_income = 0.0;
    let mut total_spending = 0.0;
    let mut total_withdrawals = 0.0;
    let mut total_taxes = 0.0;
    let mut total_federal_taxes = 0.0;
    let mut total_state_taxes = 0.0;
    let mut total_roth_conversions = 0.0;
    let mut total_aca_subsidies = 0.0;
    let mut total_medicare_premiums = 0.0;
    let mut total_irmaa_surcharges = 0.0;
    let mut total_rmd = 0.0;
    let mut total_ss_benefits = 0.0;
    let mut depletion_year: Option<i32> = None;

    for year in start_year..=end_year {
        let starting_balance: f64 = balances.iter().sum();
        // Snapshot of per-account balances before this year's growth is
        // credited — i.e. the prior year-end balances RMDs are based on.
        let starting_account_balances = balances.clone();
        let primary_age = year - birth_year;
        let spouse_age = spouse_birth_year.map(|b| year - b);
        let rmd_amount = compute_household_rmd(
            inp.accounts,
            &starting_account_balances,
            birth_year,
            primary_age,
            spouse_birth_year.zip(spouse_age),
        );

        // Income for the year, split into the tax buckets it feeds. Social
        // Security is routed to its own bucket (taxed by the provisional-income
        // worksheet); everything else contributes ordinary income per its
        // taxability (partially-taxable sources, e.g. annuities, count half).
        let mut income = 0.0;
        let mut ordinary_income_sources = 0.0;
        let mut ss_benefits = 0.0;
        for s in inp.income {
            let amt = income_for_year(s, year, inp.social_security_cola_rate);
            if amt == 0.0 {
                continue;
            }
            income += amt;
            if s.income_type == "social_security" {
                ss_benefits += amt;
            } else {
                match s.taxability.as_str() {
                    "taxable" => ordinary_income_sources += amt,
                    "partially_taxable" => ordinary_income_sources += 0.5 * amt,
                    _ => {} // tax_free
                }
            }
        }

        let spending: f64 = inp
            .spending
            .iter()
            .map(|s| {
                spending_for_year(
                    s,
                    year,
                    inp.current_year,
                    inp.inflation_rate,
                    inp.healthcare_inflation_rate,
                )
            })
            .sum();

        // Sum life events for the year while keeping each occurrence for markers.
        // A taxable inflow (e.g. a taxable account sale booked as an event) adds
        // to ordinary income.
        let mut life_events_net = 0.0;
        let mut taxable_life_inflow = 0.0;
        let mut year_events: Vec<LifeEventOccurrence> = Vec::new();
        for e in inp.life_events {
            let amount = life_event_for_year(e, year, inp.current_year, inp.inflation_rate);
            if amount != 0.0 {
                life_events_net += amount;
                if e.taxable && amount > 0.0 {
                    taxable_life_inflow += amount;
                }
                year_events.push(LifeEventOccurrence {
                    name: e.name.clone(),
                    amount: round2(amount),
                });
            }
        }

        // Credit a full year of growth before withdrawals. For taxable
        // accounts the dividend portion of that growth is currently taxable
        // (as qualified dividends) and, being reinvested, adds to cost basis;
        // the rest is unrealized appreciation.
        let mut growth = 0.0;
        let mut qualified_dividends = 0.0;
        let shock = return_shocks
            .and_then(|s| s.get((year - start_year) as usize))
            .copied()
            .unwrap_or(0.0);
        for (i, acc) in inp.accounts.iter().enumerate() {
            let start_bal = balances[i];
            let g = start_bal * (acc.expected_roi + shock) / 100.0;
            balances[i] += g;
            growth += g;
            if acc.category == "taxable" && acc.dividend_yield > 0.0 {
                let div = start_bal * acc.dividend_yield / 100.0;
                qualified_dividends += div;
                basis[i] += div;
            }
        }

        // Medicare Part B premiums (Phase 3, feature 3): a real, near-universal
        // cash need from age 65, modeled the same way as spending.
        let medicare_premiums = compute_medicare_part_b_premiums(
            inp.medicare_part_b_annual_premium,
            birth_year,
            year,
            inp.current_year,
            inp.healthcare_inflation_rate,
            spouse_birth_year,
            inp.medicare_enrollment_age,
        );

        // ---- Medicare IRMAA surcharge (Phase 3, feature 4) -----------------
        // Based on household MAGI from two years prior — the actual CMS
        // lookback — which is already settled in `annual` from earlier
        // iterations of this same projection. Only meaningful once Medicare
        // cost modeling itself is enabled (a Part B premium is configured);
        // `annual.len()` equals the number of years already completed, so the
        // year-2 entry sits two slots back.
        let irmaa_lookback_magi: Option<f64> = if inp.medicare_part_b_annual_premium > 0.0
            && annual.len() >= 2
        {
            Some(annual[annual.len() - 2].tax.magi)
        } else {
            None
        };
        let irmaa = compute_irmaa(
            &IrmaaInput {
                year,
                inflation_rate: inp.inflation_rate,
                lookback_magi: irmaa_lookback_magi,
                filing_group: IrmaaFilingGroup::from_filing_status(filing_status),
            },
            &inp.irmaa_tables,
        );
        let irmaa_enrolled_count = (year - birth_year >= inp.medicare_enrollment_age) as i32
            + spouse_birth_year
                .map_or(0, |sby| (year - sby >= inp.medicare_enrollment_age) as i32);
        let irmaa_surcharge = if irmaa.applies {
            irmaa_enrolled_count as f64
                * (irmaa.part_b_surcharge_monthly + irmaa.part_d_surcharge_monthly)
                * 12.0
        } else {
            0.0
        };

        // Cash available before tax and account draws (income + events −
        // spending − Medicare premiums − IRMAA surcharge).
        let base_cash =
            income + life_events_net - spending - medicare_premiums - irmaa_surcharge;
        let seniors = seniors_65_plus(inp.profile, filing_status, year);

        // Tax-deferred accounts owned by someone who hasn't reached the
        // "Penalty-free withdrawals" milestone yet: draws from these owe the
        // 10% early-withdrawal penalty. Joint-titled tax-deferred accounts are
        // attributed to the primary owner, matching how RMDs treat them (see
        // `tax_deferred_balance_for_owner`) — retirement accounts can't
        // legally be jointly titled. Roth conversions are unaffected (IRC
        // §72(t) exempts conversions from the penalty); they're applied via a
        // separate balance transfer below, never through `plan_withdrawals`.
        let penalized: Vec<bool> = inp
            .accounts
            .iter()
            .map(|a| {
                if a.category != "tax_deferred" {
                    return false;
                }
                let free_year = if a.owner == "spouse" {
                    spouse_penalty_free_year
                } else {
                    primary_penalty_free_year
                };
                year < free_year.unwrap_or(i32::MAX)
            })
            .collect();

        // ---- ACA subsidy eligibility (Phase 3, feature 1) -----------------
        // The premium tax credit applies before Medicare: model it whenever a
        // benchmark premium is set and the youngest household member is under
        // 65. The benchmark premium grows with the healthcare inflation rate.
        let youngest_age = match spouse_birth_year {
            Some(sb) => (year - birth_year).min(year - sb),
            None => year - birth_year,
        };
        let aca_window = inp.aca_benchmark_annual_premium > 0.0 && youngest_age < 65;
        let benchmark_this_year = inp.aca_benchmark_annual_premium
            * growth_factor(inp.healthcare_inflation_rate, year - inp.current_year);
        // The MAGI above which the credit is lost this year, if there is a cutoff.
        let aca_cutoff_magi: Option<f64> = if aca_window {
            inp.aca_tables
                .cutoff_magi(year, inp.inflation_rate, household_size)
        } else {
            None
        };

        // ---- Roth conversion (feature 6), the withdrawal plan it implies, and
        // the tax and ACA subsidy solved together. One pass, except ACA-aware
        // conversions, which are trimmed and redone if spending-driven draws
        // push MAGI over the cutoff.
        //
        // Convert traditional (tax-deferred) dollars to Roth (tax-free). The
        // converted amount is ordinary income (raising the tax that the
        // withdrawal solve must fund); the principal itself moves between
        // accounts rather than being spent. The "room" is measured against a
        // baseline that excludes spending-driven tax-deferred draws and
        // realized capital gains, so a conversion fills the bracket the user
        // targeted without depending on the (yet-unsolved) withdrawal plan:
        // fixed-ceiling targets a flat taxable income; IRMAA-aware targets the
        // next IRMAA tier two years out with a baseline that includes this
        // year's RMD, so the room shrinks as RMDs grow; ACA-aware fills the room
        // under the ACA cutoff.
        let balances_before_conversion = balances.clone();
        let mut conversion_cap = f64::INFINITY;
        // Conversions tried so far: the largest that kept MAGI under the
        // cutoff, and the smallest that didn't, as `(conversion, magi)`.
        let mut under_cutoff: Option<(f64, f64)> = None;
        let mut over_cutoff: Option<(f64, f64)> = None;
        let mut roth_conversion = 0.0;
        let mut withdrawal_order_label: &'static str = "taxable_first";
        let mut plan = WithdrawalPlan::default();
        let mut tax = TaxResult::default();
        let mut aca = AcaResult::default();
        let mut subsidy = 0.0;
        for attempt in 0..=ACA_AWARE_MAX_ATTEMPTS {
            balances.copy_from_slice(&balances_before_conversion);

            let planned_conversion = match roth_mode {
                RothConversionModeKind::IrmaaAware => plan_irmaa_aware_roth_conversion(
                    inp,
                    filing_status,
                    IrmaaFilingGroup::from_filing_status(filing_status),
                    year,
                    seniors,
                    ordinary_income_sources + taxable_life_inflow + rmd_amount,
                    qualified_dividends,
                    ss_benefits,
                    &balances,
                    &order,
                    roth_dest,
                ),
                RothConversionModeKind::FixedCeiling => plan_roth_conversion(
                    inp,
                    filing_status,
                    year,
                    seniors,
                    ordinary_income_sources + taxable_life_inflow,
                    qualified_dividends,
                    ss_benefits,
                    &balances,
                    &order,
                    roth_dest,
                ),
                RothConversionModeKind::AcaAware => plan_aca_aware_roth_conversion(
                    inp,
                    filing_status,
                    year,
                    seniors,
                    ordinary_income_sources + taxable_life_inflow + rmd_amount,
                    qualified_dividends,
                    ss_benefits,
                    &balances,
                    &order,
                    roth_dest,
                    aca_cutoff_magi,
                ),
            };
            roth_conversion = planned_conversion.min(conversion_cap);
            if roth_conversion > 0.0 {
                if let Some(dest) = roth_dest {
                    let mut remaining = roth_conversion;
                    for &i in &order {
                        if remaining <= 0.0 {
                            break;
                        }
                        if inp.accounts[i].category != "tax_deferred" {
                            continue;
                        }
                        let take = balances[i].min(remaining);
                        balances[i] -= take;
                        remaining -= take;
                    }
                    // Any tiny residual (float rounding) is folded into the deposit.
                    balances[dest] += roth_conversion - remaining;
                }
            }

            // ---- Withdrawal sequencing (Phase 2, feature 9) ----------------
            // Conventional sequencing (`order`, computed once above) always
            // draws taxable accounts fully before tax-deferred, then tax-free.
            // The tax-optimized strategy instead reorders taxable accounts by
            // ascending embedded gain (realizing the cheapest gains first)
            // and, in years where the marginal cost of realizing a gain would
            // exceed the marginal ordinary rate a tax-deferred withdrawal would
            // face, draws tax-deferred funds before taxable ones. Tax-free
            // stays last either way, preserving its growth for true shortfalls.
            let (withdrawal_order, label): (Vec<usize>, &'static str) =
                if withdrawal_strategy == WithdrawalStrategyKind::TaxOptimized
                    || withdrawal_strategy == WithdrawalStrategyKind::MaximizeIncome
                {
                    optimized_withdrawal_order(
                        inp,
                        filing_status,
                        year,
                        seniors,
                        ordinary_income_sources + taxable_life_inflow + roth_conversion,
                        qualified_dividends,
                        ss_benefits,
                        &balances,
                        &basis,
                    )
                } else {
                    (order.clone(), "taxable_first")
                };
            withdrawal_order_label = label;

            // "Don't incur penalties": drop penalized tax-deferred accounts from
            // this year's drawdown order entirely, so spending-driven
            // withdrawals and the maximize-income strategy's voluntary
            // bracket-fill draws (both of which pull from `withdrawal_order`
            // via `plan_withdrawals`) never touch them before the owner turns
            // 59½. A forced RMD never conflicts with this — RMDs never begin
            // before age 59½.
            let withdrawal_order: Vec<usize> = if inp.avoid_early_withdrawal_penalty {
                withdrawal_order.into_iter().filter(|&i| !penalized[i]).collect()
            } else {
                withdrawal_order
            };

            // ---- Bracket-filling withdrawal (Phase 2, feature 9b) ---------
            // Maximize-income only: on top of whatever spending and RMDs
            // already require, voluntarily draw tax-deferred dollars to fill
            // `bracket_fill_target` and below. This shrinks the tax-deferred
            // balance (and so future RMDs) while the withdrawal is still being
            // taxed at today's rate, rather than waiting for a larger, possibly
            // higher-bracket RMD later. The caller (`run_projection_dispatch`)
            // tries several targets as full projections and keeps whichever
            // yields the most lifetime after-tax cash.
            let deferred_floor = if withdrawal_strategy == WithdrawalStrategyKind::MaximizeIncome {
                let bracket_fill = plan_bracket_fill_withdrawal(
                    inp,
                    filing_status,
                    year,
                    seniors,
                    ordinary_income_sources + taxable_life_inflow + roth_conversion,
                    qualified_dividends,
                    ss_benefits,
                    &balances,
                    bracket_fill_target,
                    taxable_dest.is_some() || roth_dest.is_some(),
                );
                rmd_amount.max(bracket_fill)
            } else {
                rmd_amount
            };

            // Solve withdrawals, tax, and ACA subsidy together. Drawing from a
            // tax-deferred account (or converting to Roth) adds ordinary
            // income, which raises tax and MAGI: more tax raises the amount
            // that must be withdrawn, while a higher MAGI shrinks the ACA
            // subsidy that offsets it, and any RMD shortfall still forces a
            // floor on tax-deferred withdrawals regardless. A short fixed-point
            // iteration settles all three.
            plan = WithdrawalPlan::default();
            tax = TaxResult::default();
            aca = AcaResult::default();
            let mut tax_estimate = 0.0;
            subsidy = 0.0;
            for _ in 0..12 {
                // The subsidy is tax-free cash, so it reduces the withdrawal need.
                let need = (tax_estimate - base_cash - subsidy).max(0.0);
                plan = plan_withdrawals(
                    need,
                    deferred_floor,
                    &balances,
                    &basis,
                    &withdrawal_order,
                    inp.accounts,
                    &penalized,
                );
                let ordinary_income = ordinary_income_sources
                    + taxable_life_inflow
                    + roth_conversion
                    + plan.tax_deferred;
                let early_withdrawal_penalty =
                    plan.penalized_tax_deferred * EARLY_WITHDRAWAL_PENALTY_RATE;
                let input = TaxInput {
                    year,
                    filing_status,
                    inflation_rate: inp.inflation_rate,
                    seniors_65_plus: seniors,
                    ordinary_income,
                    qualified_dividends,
                    long_term_capital_gains: plan.capital_gains,
                    social_security_benefits: ss_benefits,
                    state: &inp.profile.state,
                    early_withdrawal_penalty,
                };
                tax = compute_taxes(&input, &inp.tax_tables);

                // MAGI (Phase 3, feature 2) adds back the untaxed portion of
                // Social Security to AGI; the ACA subsidy (when in its window)
                // is based on it.
                let mut new_subsidy = 0.0;
                if aca_window {
                    let magi = compute_magi(&tax, ss_benefits);
                    aca = compute_subsidy(
                        &AcaInput {
                            year,
                            inflation_rate: inp.inflation_rate,
                            magi,
                            household_size,
                            benchmark_annual_premium: benchmark_this_year,
                        },
                        &inp.aca_tables,
                    );
                    new_subsidy = aca.subsidy;
                }

                let converged = (tax.total_tax - tax_estimate).abs() < 0.5
                    && (new_subsidy - subsidy).abs() < 0.5;
                tax_estimate = tax.total_tax;
                subsidy = new_subsidy;
                if converged {
                    break;
                }
            }

            // ACA-aware: the draws that fund spending, and the conversion's own
            // tax, add to MAGI after the conversion was sized. Search for the
            // largest conversion that keeps MAGI under the cutoff: trim by the
            // excess, then interpolate between the conversions tried. The last
            // pass always runs the largest conversion known to be under.
            if roth_mode == RothConversionModeKind::AcaAware && roth_conversion > 0.0 && attempt < ACA_AWARE_MAX_ATTEMPTS {
                if let Some(cutoff) = aca_cutoff_magi {
                    let target = cutoff - ACA_AWARE_CONVERSION_BUFFER;
                    let now = (roth_conversion, compute_magi(&tax, ss_benefits));
                    if now.1 > target + ACA_AWARE_TOLERANCE {
                        over_cutoff = Some(now);
                    } else {
                        under_cutoff = Some(now);
                        if over_cutoff.is_none() || target - now.1 <= ACA_AWARE_TOLERANCE {
                            break;
                        }
                    }
                    conversion_cap = if attempt == ACA_AWARE_MAX_ATTEMPTS - 1 {
                        under_cutoff.map_or(0.0, |u| u.0)
                    } else if let (Some(under), Some(over)) = (under_cutoff, over_cutoff) {
                        if over.1 > under.1 {
                            under.0 + (target - under.1) * (over.0 - under.0) / (over.1 - under.1)
                        } else {
                            (roth_conversion - (now.1 - target)).max(0.0)
                        }
                    } else {
                        (roth_conversion - (now.1 - target)).max(0.0)
                    };
                    continue;
                }
            }
            break;
        }

        // Apply the settled withdrawal plan to real balances and basis.
        let mut withdrawals = 0.0;
        let mut contributions = 0.0;
        let mut shortfall = 0.0;
        let mut withdrawal_sources: Vec<QuarterWithdrawal> = Vec::new();
        for &(idx, take) in &plan.takes {
            let bal_before = balances[idx];
            balances[idx] -= take;
            withdrawals += take;
            if inp.accounts[idx].category == "taxable" && bal_before > 0.0 {
                // Realizing part of the account reduces basis proportionally.
                basis[idx] = (basis[idx] * (balances[idx] / bal_before)).max(0.0);
            }
            let acc = &inp.accounts[idx];
            let source = QuarterWithdrawal {
                account_id: acc.id.clone(),
                account_name: acc.name.clone(),
                category: acc.category.clone(),
                amount: take,
            };
            if year == start_year {
                first_year_withdrawals.push(source.clone());
            }
            withdrawal_sources.push(source);
        }
        if plan.shortfall > 0.01 {
            shortfall = plan.shortfall;
            if depletion_year.is_none() {
                depletion_year = Some(year);
            }
        }

        // Any cash left after covering spending and tax is reinvested. This
        // also captures the after-tax proceeds of a forced RMD draw that
        // exceeded what spending actually needed (`withdrawals` can now run
        // ahead of the spending/tax gap `base_cash - tax.total_tax` alone
        // would imply), plus the ACA subsidy, which is tax-free cash that adds
        // to the surplus.
        let surplus = base_cash + subsidy + withdrawals - tax.total_tax;
        if surplus > 0.0 {
            if let Some(idx) = reinvest_target {
                balances[idx] += surplus;
                contributions = surplus;
                if inp.accounts[idx].category == "taxable" {
                    basis[idx] += surplus; // after-tax reinvestment adds basis
                }
            }
            // With no drawdown account to hold it, surplus simply goes unmodeled.
        }

        let ending_balance: f64 = balances.iter().sum();
        let account_balances: Vec<YearAccountBalance> = inp
            .accounts
            .iter()
            .zip(starting_account_balances.iter())
            .zip(balances.iter())
            .map(|((acc, &start), &end)| YearAccountBalance {
                account_id: acc.id.clone(),
                account_name: acc.name.clone(),
                category: acc.category.clone(),
                starting_balance: round2(start),
                ending_balance: round2(end),
                change: round2(end - start),
            })
            .collect();

        let ordinary_income_total =
            ordinary_income_sources + taxable_life_inflow + roth_conversion + plan.tax_deferred;

        total_income += income;
        total_spending += spending;
        total_withdrawals += withdrawals;
        total_taxes += tax.total_tax;
        total_federal_taxes += tax.federal_tax;
        total_state_taxes += tax.state_tax;
        total_roth_conversions += roth_conversion;
        total_aca_subsidies += subsidy;
        total_medicare_premiums += medicare_premiums;
        total_irmaa_surcharges += irmaa_surcharge;
        total_rmd += rmd_amount;
        total_ss_benefits += ss_benefits;
        if year == start_year {
            first_year_tax = tax.total_tax;
        }

        // MAGI (Phase 3, feature 2), tracked every year regardless of ACA
        // eligibility so it can be forecast across the whole plan.
        let magi = compute_magi(&tax, ss_benefits);

        annual.push(YearProjection {
            year,
            primary_age,
            spouse_age,
            starting_balance: round2(starting_balance),
            income: round2(income),
            spending: round2(spending),
            life_events_net: round2(life_events_net),
            life_events: year_events,
            milestones: milestones.remove(&year).unwrap_or_default(),
            growth: round2(growth),
            withdrawals: round2(withdrawals),
            rmd_amount: round2(rmd_amount),
            medicare_premiums: round2(medicare_premiums),
            irmaa_surcharge: round2(irmaa_surcharge),
            contributions: round2(contributions),
            roth_conversion: round2(roth_conversion),
            taxes: round2(tax.total_tax),
            tax: YearTax {
                ordinary_income: round2(ordinary_income_total),
                qualified_dividends: round2(qualified_dividends),
                capital_gains: round2(plan.capital_gains),
                social_security_benefits: round2(ss_benefits),
                taxable_social_security: round2(tax.taxable_social_security),
                adjusted_gross_income: round2(tax.adjusted_gross_income),
                magi: round2(magi),
                standard_deduction: round2(tax.standard_deduction),
                taxable_income: round2(tax.taxable_income),
                federal_ordinary_tax: round2(tax.federal_ordinary_tax),
                federal_capital_gains_tax: round2(tax.federal_capital_gains_tax),
                early_withdrawal_penalty: round2(tax.early_withdrawal_penalty),
                federal_tax: round2(tax.federal_tax),
                state_taxable_income: round2(tax.state_taxable_income),
                state_standard_deduction: round2(tax.state_standard_deduction),
                state_tax: round2(tax.state_tax),
                state_marginal_rate: tax.state_marginal_rate,
                property_tax: 0.0,
                total_tax: round2(tax.total_tax),
                effective_rate: tax.effective_rate,
                marginal_rate: tax.marginal_rate,
            },
            withdrawal_order: withdrawal_order_label.to_string(),
            aca: YearAca {
                eligible: aca.eligible,
                magi: round2(aca.magi),
                federal_poverty_line: round2(aca.federal_poverty_line),
                fpl_percent: round2(aca.fpl_percent),
                applicable_percentage: aca.applicable_percentage,
                expected_contribution: round2(aca.expected_contribution),
                benchmark_premium: round2(aca.benchmark_premium),
                subsidy: round2(subsidy),
            },
            irmaa: YearIrmaa {
                applies: irmaa.applies,
                has_lookback_data: irmaa.has_lookback_data,
                lookback_year: year - 2,
                lookback_magi: round2(irmaa.lookback_magi),
                part_b_surcharge_monthly: irmaa.part_b_surcharge_monthly,
                part_d_surcharge_monthly: irmaa.part_d_surcharge_monthly,
                enrolled_count: irmaa_enrolled_count,
                total_surcharge: round2(irmaa_surcharge),
            },
            ending_balance: round2(ending_balance),
            account_balances,
            shortfall: round2(shortfall),
            withdrawal_sources,
        });
    }

    let projected_ending_balance = annual.last().map(|y| y.ending_balance).unwrap_or(0.0);
    let first = annual.first();
    let quarterly = build_quarterly(
        start_year,
        first.map(|y| y.income).unwrap_or(0.0),
        first.map(|y| y.spending).unwrap_or(0.0),
        first_year_tax,
        &first_year_withdrawals,
    );
    let estimated_taxes = build_estimated_taxes(start_year, first_year_tax);

    ProjectionResponse {
        current_year: inp.current_year,
        start_year,
        end_year,
        assumptions: ProjectionAssumptions {
            inflation_rate: inp.inflation_rate,
            investment_return_rate: inp.investment_return_rate,
            healthcare_inflation_rate: inp.healthcare_inflation_rate,
            social_security_cola_rate: inp.social_security_cola_rate,
            roth_conversion_ceiling: inp.roth_conversion_ceiling,
            roth_conversion_start_year: inp.roth_conversion_start_year,
            roth_conversion_end_year: inp.roth_conversion_end_year,
            withdrawal_strategy: inp.withdrawal_strategy.clone(),
            aca_benchmark_annual_premium: inp.aca_benchmark_annual_premium,
            medicare_part_b_annual_premium: inp.medicare_part_b_annual_premium,
            avoid_early_withdrawal_penalty: inp.avoid_early_withdrawal_penalty,
            medicare_enrollment_age: inp.medicare_enrollment_age,
            roth_conversion_mode: inp.roth_conversion_mode.clone(),
            is_default: inp.assumptions_are_default,
        },
        summary: ProjectionSummary {
            current_net_worth: round2(current_net_worth),
            projected_ending_balance,
            total_lifetime_income: round2(total_income),
            total_lifetime_spending: round2(total_spending),
            total_lifetime_withdrawals: round2(total_withdrawals),
            total_lifetime_taxes: round2(total_taxes),
            total_lifetime_federal_taxes: round2(total_federal_taxes),
            total_lifetime_state_taxes: round2(total_state_taxes),
            total_lifetime_roth_conversions: round2(total_roth_conversions),
            total_lifetime_aca_subsidies: round2(total_aca_subsidies),
            total_lifetime_medicare_premiums: round2(total_medicare_premiums),
            total_lifetime_irmaa_surcharges: round2(total_irmaa_surcharges),
            total_lifetime_rmd: round2(total_rmd),
            total_lifetime_social_security: round2(total_ss_benefits),
            depletion_year,
        },
        annual,
        quarterly,
        estimated_taxes,
    }
}

/// Fraction of a balance that is embedded (unrealized) gain, clamped to
/// `[0, 1]`. Zero for a zero (or negative) balance.
fn gain_fraction(balance: f64, basis: f64) -> f64 {
    if balance > 0.0 {
        ((balance - basis) / balance).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Category priority for the tax-optimized strategy (Phase 2, feature 9): the
/// same tiers as [`category_priority`], but with taxable and tax-deferred
/// swapped when `swap` is true. Tax-free always stays last, since it is never
/// cheaper at the margin to draw and doing so forfeits its tax-free growth.
fn optimized_priority(category: &str, swap: bool) -> u8 {
    match category {
        "taxable" => {
            if swap {
                1
            } else {
                0
            }
        }
        "tax_deferred" => {
            if swap {
                0
            } else {
                1
            }
        }
        "tax_free" => 2,
        _ => 3, // unreachable — "other" accounts are filtered out before sorting
    }
}

/// Decide this year's withdrawal order for the tax-optimized strategy
/// (Phase 2, feature 9). Reorders taxable accounts by ascending embedded gain
/// (so the cheapest gains are realized first) and, when the marginal cost of
/// realizing a taxable gain exceeds the marginal ordinary rate a tax-deferred
/// withdrawal would face, draws tax-deferred funds before taxable ones. The
/// comparison uses a baseline tax position (this year's income *before* any
/// spending-driven withdrawal), matching how [`plan_roth_conversion`] measures
/// bracket room. Returns the reordered account-index list plus a short
/// machine-readable label describing which tier went first.
#[allow(clippy::too_many_arguments)]
fn optimized_withdrawal_order(
    inp: &ProjectionInputs,
    filing_status: FilingStatusKind,
    year: i32,
    seniors: u8,
    baseline_ordinary_income: f64,
    qualified_dividends: f64,
    ss_benefits: f64,
    balances: &[f64],
    basis: &[f64],
) -> (Vec<usize>, &'static str) {
    let baseline = compute_taxes(
        &TaxInput {
            early_withdrawal_penalty: 0.0,
            year,
            filing_status,
            inflation_rate: inp.inflation_rate,
            seniors_65_plus: seniors,
            ordinary_income: baseline_ordinary_income,
            qualified_dividends,
            long_term_capital_gains: 0.0,
            social_security_benefits: ss_benefits,
            state: &inp.profile.state,
        },
        &inp.tax_tables,
    );
    // The next dollar of realized gain stacks on top of ordinary taxable
    // income (qualified dividends are already stacked in the baseline).
    let ordinary_taxable = (baseline.taxable_income - qualified_dividends).max(0.0);
    let cg_marginal = inp.tax_tables.capital_gains_marginal_rate(
        filing_status,
        year,
        inp.inflation_rate,
        ordinary_taxable,
    );

    let taxable_balance: f64 = (0..inp.accounts.len())
        .filter(|&i| inp.accounts[i].category == "taxable")
        .map(|i| balances[i])
        .sum();
    let taxable_basis: f64 = (0..inp.accounts.len())
        .filter(|&i| inp.accounts[i].category == "taxable")
        .map(|i| basis[i])
        .sum();
    let tax_deferred_balance: f64 = (0..inp.accounts.len())
        .filter(|&i| inp.accounts[i].category == "tax_deferred")
        .map(|i| balances[i])
        .sum();
    let blended_gain_fraction = gain_fraction(taxable_balance, taxable_basis);
    let taxable_marginal_cost = cg_marginal * blended_gain_fraction;

    // Only swap when there is an actual choice to make between the two tiers.
    let swap = taxable_balance > 0.0
        && tax_deferred_balance > 0.0
        && taxable_marginal_cost > baseline.marginal_rate;

    let mut order: Vec<usize> = (0..inp.accounts.len())
        .filter(|&i| category_priority(&inp.accounts[i].category).is_some())
        .collect();
    order.sort_by(|&a, &b| {
        let pa = optimized_priority(&inp.accounts[a].category, swap);
        let pb = optimized_priority(&inp.accounts[b].category, swap);
        pa.cmp(&pb)
            .then_with(|| {
                if inp.accounts[a].category == "taxable" && inp.accounts[b].category == "taxable" {
                    // Ascending embedded gain: realize the cheapest gains first.
                    gain_fraction(balances[a], basis[a])
                        .partial_cmp(&gain_fraction(balances[b], basis[b]))
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    balances[b]
                        .partial_cmp(&balances[a])
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            })
            .then(a.cmp(&b))
    });

    let label = if swap {
        "tax_deferred_first"
    } else {
        "taxable_first"
    };
    (order, label)
}

/// Decide how much to convert from tax-deferred to Roth for a single year
/// (feature 6). Returns 0 when the strategy is off, out of its year window, has
/// no tax-free destination account, or the ceiling is already met by other
/// income. The result is capped by the available tax-deferred balance.
#[allow(clippy::too_many_arguments)]
fn plan_roth_conversion(
    inp: &ProjectionInputs,
    filing_status: FilingStatusKind,
    year: i32,
    seniors: u8,
    baseline_ordinary_income: f64,
    qualified_dividends: f64,
    ss_benefits: f64,
    balances: &[f64],
    order: &[usize],
    roth_dest: Option<usize>,
) -> f64 {
    if inp.roth_conversion_ceiling <= 0.0 || roth_dest.is_none() {
        return 0.0;
    }
    let in_window = inp.roth_conversion_start_year.map_or(true, |s| year >= s)
        && inp.roth_conversion_end_year.map_or(true, |e| year <= e);
    if !in_window {
        return 0.0;
    }
    let deferred_available: f64 = order
        .iter()
        .filter(|&&i| inp.accounts[i].category == "tax_deferred")
        .map(|&i| balances[i])
        .sum();
    if deferred_available <= 0.0 {
        return 0.0;
    }

    // Baseline (no conversion, no spending-driven deferred draws, no realized
    // gains). A converted dollar adds a dollar of AGI, but only lifts *taxable*
    // income once the standard deduction is used up — so target the ceiling on
    // an AGI basis: convert until AGI reaches (ceiling + deduction). This lands
    // taxable income at the ceiling. Taxable Social Security can rise slightly
    // as AGI grows, a second-order effect left as an approximation.
    let baseline = compute_taxes(
        &TaxInput {
            early_withdrawal_penalty: 0.0,
            year,
            filing_status,
            inflation_rate: inp.inflation_rate,
            seniors_65_plus: seniors,
            ordinary_income: baseline_ordinary_income,
            qualified_dividends,
            long_term_capital_gains: 0.0,
            social_security_benefits: ss_benefits,
            state: &inp.profile.state,
        },
        &inp.tax_tables,
    );
    let target_agi = inp.roth_conversion_ceiling + baseline.standard_deduction;
    let room = (target_agi - baseline.adjusted_gross_income).max(0.0);
    room.min(deferred_available)
}

/// A small buffer below an IRMAA bracket threshold the "IRMAA-aware" Roth
/// conversion mode targets, so the result lands strictly under the tier
/// rather than exactly on its edge (where the surcharge would apply).
const IRMAA_AWARE_CONVERSION_BUFFER: f64 = 1.0;

/// Decide how much to convert from tax-deferred to Roth for a single year
/// under the "IRMAA-aware" mode (feature 6): unlike [`plan_roth_conversion`]'s
/// flat AGI ceiling, this sizes the conversion to stay just under the
/// household's next Medicare IRMAA bracket two years out — the actual CMS
/// MAGI lookback — so a conversion made this year doesn't push the household
/// into a higher-surcharge tier once it's assessed. `baseline_ordinary_income`
/// is expected to include this year's RMD (unlike the fixed-ceiling mode's
/// baseline), so the room available shrinks as RMDs grow and reaches zero
/// once RMDs alone meet or exceed the target — the "timing around RMDs" half
/// of the feature. A nonzero `roth_conversion_ceiling`, if also configured,
/// applies as an additional upper bound. Returns 0 when out of the
/// conversion window or there's no tax-free destination account; capped by
/// the available tax-deferred balance.
#[allow(clippy::too_many_arguments)]
fn plan_irmaa_aware_roth_conversion(
    inp: &ProjectionInputs,
    filing_status: FilingStatusKind,
    filing_group: IrmaaFilingGroup,
    year: i32,
    seniors: u8,
    baseline_ordinary_income: f64,
    qualified_dividends: f64,
    ss_benefits: f64,
    balances: &[f64],
    order: &[usize],
    roth_dest: Option<usize>,
) -> f64 {
    if roth_dest.is_none() {
        return 0.0;
    }
    let in_window = inp.roth_conversion_start_year.map_or(true, |s| year >= s)
        && inp.roth_conversion_end_year.map_or(true, |e| year <= e);
    if !in_window {
        return 0.0;
    }
    let deferred_available: f64 = order
        .iter()
        .filter(|&&i| inp.accounts[i].category == "tax_deferred")
        .map(|&i| balances[i])
        .sum();
    if deferred_available <= 0.0 {
        return 0.0;
    }

    let baseline = compute_taxes(
        &TaxInput {
            early_withdrawal_penalty: 0.0,
            year,
            filing_status,
            inflation_rate: inp.inflation_rate,
            seniors_65_plus: seniors,
            ordinary_income: baseline_ordinary_income,
            qualified_dividends,
            long_term_capital_gains: 0.0,
            social_security_benefits: ss_benefits,
            state: &inp.profile.state,
        },
        &inp.tax_tables,
    );
    let baseline_magi = compute_magi(&baseline, ss_benefits);

    // This year's MAGI is looked back at for IRMAA in `year + 2` (the actual
    // CMS lookback) — target just under whatever tier applies then.
    let irmaa_room = match inp
        .irmaa_tables
        .next_threshold(filing_group, year + 2, inp.inflation_rate, baseline_magi)
    {
        Some(threshold) => {
            let target_magi = threshold - IRMAA_AWARE_CONVERSION_BUFFER;
            // MAGI = AGI + untaxed Social Security; convert the MAGI target
            // back to AGI space using the same untaxed-SS offset (a fixed
            // amount, independent of the conversion size, matching the
            // approximation `plan_roth_conversion` already makes).
            let untaxed_ss = ss_benefits - baseline.taxable_social_security;
            let target_agi = target_magi - untaxed_ss;
            (target_agi - baseline.adjusted_gross_income).max(0.0)
        }
        // Already at/above the top bracket: no further IRMAA tier to avoid.
        None => f64::INFINITY,
    };

    let room = if inp.roth_conversion_ceiling > 0.0 {
        let target_agi_ceiling = inp.roth_conversion_ceiling + baseline.standard_deduction;
        let ceiling_room = (target_agi_ceiling - baseline.adjusted_gross_income).max(0.0);
        irmaa_room.min(ceiling_room)
    } else {
        irmaa_room
    };
    room.min(deferred_available)
}

/// Tax position for the year before any spending-driven withdrawal: this
/// year's income without conversion draws or realized gains. The room the
/// conversion planners fill is measured against it.
#[allow(clippy::too_many_arguments)]
fn baseline_tax(
    inp: &ProjectionInputs,
    filing_status: FilingStatusKind,
    year: i32,
    seniors: u8,
    baseline_ordinary_income: f64,
    qualified_dividends: f64,
    ss_benefits: f64,
) -> TaxResult {
    compute_taxes(
        &TaxInput {
            early_withdrawal_penalty: 0.0,
            year,
            filing_status,
            inflation_rate: inp.inflation_rate,
            seniors_65_plus: seniors,
            ordinary_income: baseline_ordinary_income,
            qualified_dividends,
            long_term_capital_gains: 0.0,
            social_security_benefits: ss_benefits,
            state: &inp.profile.state,
        },
        &inp.tax_tables,
    )
}

/// Whether `year` is inside the configured Roth conversion window.
fn in_conversion_window(inp: &ProjectionInputs, year: i32) -> bool {
    inp.roth_conversion_start_year.map_or(true, |s| year >= s)
        && inp.roth_conversion_end_year.map_or(true, |e| year <= e)
}

/// Tax-deferred dollars available to convert.
fn deferred_available(inp: &ProjectionInputs, balances: &[f64], order: &[usize]) -> f64 {
    order
        .iter()
        .filter(|&&i| inp.accounts[i].category == "tax_deferred")
        .map(|&i| balances[i])
        .sum()
}

/// ACA-aware Roth conversion: convert up to the room under the ACA cutoff
/// (`cutoff_magi`), given the income already in the year (RMD, dividends,
/// Social Security). A nonzero ceiling is an extra cap. In years the cutoff
/// doesn't apply (`None`), fill the flat ceiling instead. The room ignores
/// spending-driven draws; the year solver trims the conversion for them.
#[allow(clippy::too_many_arguments)]
fn plan_aca_aware_roth_conversion(
    inp: &ProjectionInputs,
    filing_status: FilingStatusKind,
    year: i32,
    seniors: u8,
    baseline_ordinary_income: f64,
    qualified_dividends: f64,
    ss_benefits: f64,
    balances: &[f64],
    order: &[usize],
    roth_dest: Option<usize>,
    cutoff_magi: Option<f64>,
) -> f64 {
    if roth_dest.is_none() || !in_conversion_window(inp, year) {
        return 0.0;
    }
    let available = deferred_available(inp, balances, order);
    if available <= 0.0 {
        return 0.0;
    }
    let base = baseline_tax(
        inp,
        filing_status,
        year,
        seniors,
        baseline_ordinary_income,
        qualified_dividends,
        ss_benefits,
    );
    let ceiling_room = if inp.roth_conversion_ceiling > 0.0 {
        Some(
            (inp.roth_conversion_ceiling + base.standard_deduction - base.adjusted_gross_income)
                .max(0.0),
        )
    } else {
        None
    };

    let room = match cutoff_magi {
        Some(cutoff) => {
            // MAGI is AGI plus untaxed Social Security; convert the target to AGI.
            let untaxed_ss = ss_benefits - base.taxable_social_security;
            let target_agi = cutoff - ACA_AWARE_CONVERSION_BUFFER - untaxed_ss;
            let aca_room = (target_agi - base.adjusted_gross_income).max(0.0);
            ceiling_room.map_or(aca_room, |c| aca_room.min(c))
        }
        None => match ceiling_room {
            Some(c) => c,
            None => return 0.0,
        },
    };
    room.min(available)
}

/// Size this year's voluntary bracket-filling withdrawal for the
/// maximize-income strategy (feature 9b): the room left between this year's
/// baseline ordinary taxable income and the top of `target_bracket`, capped
/// by the available tax-deferred balance. `None` (or a target already at or
/// below the top the household already occupies) draws nothing. The baseline
/// excludes withdrawal-driven income (the same approximation
/// [`plan_roth_conversion`] makes) since the actual withdrawal plan is what
/// this figure feeds into.
#[allow(clippy::too_many_arguments)]
fn plan_bracket_fill_withdrawal(
    inp: &ProjectionInputs,
    filing_status: FilingStatusKind,
    year: i32,
    seniors: u8,
    baseline_ordinary_income: f64,
    qualified_dividends: f64,
    ss_benefits: f64,
    balances: &[f64],
    target_bracket: Option<usize>,
    has_reinvest_destination: bool,
) -> f64 {
    let Some(target_bracket) = target_bracket else {
        return 0.0;
    };
    // Without a taxable or tax-free account to hold the proceeds, this
    // withdrawal would just be reinvested back into tax-deferred — taxing the
    // same dollar today for nothing, rather than actually getting it out of
    // future RMDs' reach. Nothing to gain, so don't draw it.
    if !has_reinvest_destination {
        return 0.0;
    }
    let deferred_available: f64 = (0..inp.accounts.len())
        .filter(|&i| inp.accounts[i].category == "tax_deferred")
        .map(|i| balances[i])
        .sum();
    if deferred_available <= 0.0 {
        return 0.0;
    }

    let baseline = compute_taxes(
        &TaxInput {
            early_withdrawal_penalty: 0.0,
            year,
            filing_status,
            inflation_rate: inp.inflation_rate,
            seniors_65_plus: seniors,
            ordinary_income: baseline_ordinary_income,
            qualified_dividends,
            long_term_capital_gains: 0.0,
            social_security_benefits: ss_benefits,
            state: &inp.profile.state,
        },
        &inp.tax_tables,
    );
    let ordinary_taxable = (baseline.taxable_income - qualified_dividends).max(0.0);
    let target_ceiling = inp
        .tax_tables
        .ordinary_bracket_top(filing_status, year, inp.inflation_rate, target_bracket)
        .unwrap_or(f64::INFINITY);
    let room = (target_ceiling - ordinary_taxable).max(0.0);
    room.min(deferred_available)
}

/// Build the current-year estimated tax installments (feature 7): the year's
/// projected liability split into four equal IRS Form 1040-ES payments, each
/// with its standard due date. The final installment absorbs any rounding
/// remainder so the four sum to the year's total.
fn build_estimated_taxes(tax_year: i32, total_tax: f64) -> EstimatedTaxes {
    let total = round2(total_tax.max(0.0));
    let quarterly = round2(total / 4.0);
    let schedule = [
        ("Q1", "Jan – Mar", format!("{tax_year}-04-15")),
        ("Q2", "Apr – May", format!("{tax_year}-06-15")),
        ("Q3", "Jun – Aug", format!("{tax_year}-09-15")),
        ("Q4", "Sep – Dec", format!("{}-01-15", tax_year + 1)),
    ];
    let payments: Vec<EstimatedTaxPayment> = schedule
        .iter()
        .enumerate()
        .map(|(i, (q, period, due))| {
            // The last voucher takes the remainder so the four reconcile.
            let amount = if i == 3 {
                round2(total - quarterly * 3.0)
            } else {
                quarterly
            };
            EstimatedTaxPayment {
                label: format!("{tax_year} {q}"),
                period: (*period).to_string(),
                due_date: due.clone(),
                amount,
            }
        })
        .collect();

    EstimatedTaxes {
        tax_year,
        total,
        note: format!(
            "Four equal installments of your projected {tax_year} tax, due on the IRS estimated-tax dates."
        ),
        payments,
    }
}

/// Split the first projection year's totals evenly across four quarters to form
/// the actionable near-term withdrawal schedule (feature 9).
fn build_quarterly(
    year: i32,
    year_income: f64,
    year_spending: f64,
    year_tax: f64,
    year_withdrawals: &[QuarterWithdrawal],
) -> Vec<QuarterProjection> {
    (1..=4)
        .map(|q| {
            let withdrawals: Vec<QuarterWithdrawal> = year_withdrawals
                .iter()
                .map(|w| QuarterWithdrawal {
                    account_id: w.account_id.clone(),
                    account_name: w.account_name.clone(),
                    category: w.category.clone(),
                    amount: round2(w.amount / 4.0),
                })
                .collect();
            let total_withdrawal = round2(withdrawals.iter().map(|w| w.amount).sum());
            QuarterProjection {
                label: format!("{year} Q{q}"),
                year,
                quarter: q,
                income: round2(year_income / 4.0),
                spending: round2(year_spending / 4.0),
                estimated_tax: round2(year_tax / 4.0),
                total_withdrawal,
                withdrawals,
            }
        })
        .collect()
}

/// A settled per-year withdrawal plan produced by [`plan_withdrawals`]. It
/// records which accounts to draw from and classifies the draw for tax: how
/// much came from tax-deferred accounts (ordinary income) and the realized
/// long-term capital gains from taxable accounts.
#[derive(Default)]
struct WithdrawalPlan {
    /// `(account index, amount)` pairs to apply.
    takes: Vec<(usize, f64)>,
    /// Total drawn from tax-deferred accounts (fully ordinary income).
    tax_deferred: f64,
    /// Portion of `tax_deferred` drawn from an account whose owner hasn't
    /// reached the age-59½ milestone yet — subject to the 10% early-
    /// withdrawal penalty.
    penalized_tax_deferred: f64,
    /// Realized long-term capital gains from taxable-account draws.
    capital_gains: f64,
    /// Amount of `need` that could not be raised (accounts exhausted).
    shortfall: f64,
}

/// Simulate raising `need` in cash from accounts in drawdown `order`, without
/// mutating anything, then top up tax-deferred withdrawals to `rmd_floor` if
/// spending didn't already draw that much (RMD module) — that excess isn't
/// needed for spending but must still be withdrawn and taxed; the caller
/// reinvests it as surplus. Taxable-account gains are realized proportionally
/// to each account's embedded gain (balance − basis). This is called
/// repeatedly during the per-year tax fixed-point, so it reads from snapshots
/// and returns a plan the caller applies once it converges.
fn plan_withdrawals(
    need: f64,
    rmd_floor: f64,
    balances: &[f64],
    basis: &[f64],
    order: &[usize],
    accounts: &[Account],
    penalized: &[bool],
) -> WithdrawalPlan {
    let mut plan = WithdrawalPlan::default();
    let mut raised = vec![0.0_f64; balances.len()];

    let mut remaining = need.max(0.0);
    for &idx in order {
        if remaining <= 0.0 {
            break;
        }
        let take = balances[idx].min(remaining);
        if take <= 0.0 {
            continue;
        }
        remaining -= take;
        raised[idx] += take;
        if accounts[idx].category == "tax_deferred" {
            plan.tax_deferred += take;
            if penalized[idx] {
                plan.penalized_tax_deferred += take;
            }
        }
    }
    plan.shortfall = remaining.max(0.0);

    if rmd_floor > plan.tax_deferred {
        let mut extra = rmd_floor - plan.tax_deferred;
        for &idx in order {
            if extra <= 0.0 {
                break;
            }
            if accounts[idx].category != "tax_deferred" {
                continue;
            }
            let avail = balances[idx] - raised[idx];
            let take = avail.min(extra);
            if take <= 0.0 {
                continue;
            }
            raised[idx] += take;
            plan.tax_deferred += take;
            if penalized[idx] {
                plan.penalized_tax_deferred += take;
            }
            extra -= take;
        }
    }

    for &idx in order {
        if raised[idx] <= 0.0 {
            continue;
        }
        plan.takes.push((idx, raised[idx]));
        if accounts[idx].category == "taxable" {
            plan.capital_gains += raised[idx] * gain_fraction(balances[idx], basis[idx]);
        }
    }

    plan
}

/// Count taxpayers age 65+ at year-end for the additional standard deduction.
/// The spouse is only counted on a joint return.
fn seniors_65_plus(profile: &Profile, filing: FilingStatusKind, year: i32) -> u8 {
    let mut n = 0u8;
    if year - profile.date_of_birth.year() >= 65 {
        n += 1;
    }
    let joint = matches!(
        filing,
        FilingStatusKind::MarriedFilingJointly | FilingStatusKind::QualifyingWidow
    );
    if joint {
        if let Some(sdob) = profile.spouse_date_of_birth {
            if year - sdob.year() >= 65 {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

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

    fn spending(name: &str, category: &str, amount: f64, frequency: &str, inflation: bool) -> SpendingItem {
        SpendingItem {
            id: name.into(),
            user_id: "u1".into(),
            name: name.into(),
            category: category.into(),
            amount,
            frequency: frequency.into(),
            inflation_adjusted: inflation,
            start_year: None,
            end_year: None,
            notes: None,
            created_at: ts(),
            updated_at: ts(),
        }
    }

    fn income(name: &str, amount: f64, frequency: &str, start: NaiveDate, cola: bool, growth: f64) -> IncomeSource {
        IncomeSource {
            id: name.into(),
            user_id: "u1".into(),
            name: name.into(),
            income_type: "pension".into(),
            owner: "self".into(),
            amount,
            frequency: frequency.into(),
            start_date: start,
            end_date: None,
            growth_rate: growth,
            cola,
            taxability: "taxable".into(),
            notes: None,
            created_at: ts(),
            updated_at: ts(),
        }
    }

    /// An annual, taxable income source of a specific type (e.g. social_security).
    fn income_typed(name: &str, income_type: &str, amount: f64, start: NaiveDate) -> IncomeSource {
        let mut i = income(name, amount, "annual", start, false, 0.0);
        i.income_type = income_type.into();
        i
    }

    fn life_event(name: &str, date: NaiveDate, direction: &str, amount: f64) -> LifeEvent {
        LifeEvent {
            id: name.into(),
            user_id: "u1".into(),
            name: name.into(),
            event_type: "other".into(),
            event_date: date,
            direction: direction.into(),
            amount,
            taxable: false,
            inflation_adjusted: false,
            recurrence: "one_time".into(),
            end_date: None,
            notes: None,
            created_at: ts(),
            updated_at: ts(),
        }
    }

    fn base_inputs<'a>(
        profile: &'a Profile,
        accounts: &'a [Account],
        income: &'a [IncomeSource],
        spending: &'a [SpendingItem],
        life_events: &'a [LifeEvent],
    ) -> ProjectionInputs<'a> {
        ProjectionInputs {
            current_year: 2026,
            profile,
            accounts,
            income,
            spending,
            life_events,
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
            avoid_early_withdrawal_penalty: false,
            medicare_enrollment_age: 65,
            roth_conversion_mode: "fixed_ceiling".to_string(),
            tax_tables: TaxTables::default_2025(),
            aca_tables: crate::aca::AcaTables::default_2025(),
            irmaa_tables: IrmaaTables::default_2025(),
        }
    }

    #[test]
    fn horizon_runs_from_current_year_to_life_expectancy() {
        let p = profile(1960, 90); // ends 2050
        let out = run_projection(&base_inputs(&p, &[], &[], &[], &[]));
        assert_eq!(out.start_year, 2026);
        assert_eq!(out.end_year, 2050);
        assert_eq!(out.annual.len(), 2050 - 2026 + 1);
        assert_eq!(out.annual[0].primary_age, 2026 - 1960);
    }

    #[test]
    fn life_events_are_listed_per_year_with_signed_amounts() {
        let p = profile(1960, 2036 - 1960);
        let events = [
            life_event("Inheritance", date(2035, 5, 1), "inflow", 200_000.0),
            life_event("Buy RV", date(2035, 8, 1), "outflow", 80_000.0),
            life_event("Downsize", date(2030, 1, 1), "inflow", 150_000.0),
        ];
        let out = run_projection(&base_inputs(&p, &[], &[], &[], &events));
        let rows = |yr: i32| out.annual.iter().find(|r| r.year == yr).unwrap();

        // 2035 carries the inheritance (+) and the RV purchase (−).
        let y2035 = rows(2035);
        assert_eq!(y2035.life_events.len(), 2);
        let inheritance = y2035
            .life_events
            .iter()
            .find(|e| e.name == "Inheritance")
            .unwrap();
        assert_eq!(inheritance.amount, 200_000.0);
        let rv = y2035.life_events.iter().find(|e| e.name == "Buy RV").unwrap();
        assert_eq!(rv.amount, -80_000.0);

        // A year with no events lists none.
        assert!(rows(2033).life_events.is_empty());
        // The 2030 downsize appears only in its year.
        assert_eq!(rows(2030).life_events.len(), 1);
    }

    #[test]
    fn milestones_land_in_the_right_years() {
        // Born 1965: Medicare at 65 -> 2030, RMDs at 75 -> 2040, SS at 62 -> 2027.
        let p = profile(1965, 2055 - 1965);
        let out = run_projection(&base_inputs(&p, &[], &[], &[], &[]));
        let labels = |yr: i32| -> Vec<String> {
            out.annual
                .iter()
                .find(|r| r.year == yr)
                .unwrap()
                .milestones
                .iter()
                .map(|m| m.label.clone())
                .collect()
        };
        assert!(labels(2030).contains(&"Medicare eligibility".to_string()));
        assert!(labels(2040).contains(&"RMDs begin".to_string()));
        assert!(labels(2027).contains(&"Social Security eligibility".to_string()));
        // A year with no milestone has an empty list.
        assert!(labels(2028).is_empty());
    }

    #[test]
    fn spouse_milestones_are_included_and_labelled() {
        let mut p = profile(1965, 2055 - 1965);
        p.spouse_date_of_birth = Some(date(1968, 6, 1)); // spouse Medicare 65 -> 2033
        p.spouse_life_expectancy = Some(90);
        let out = run_projection(&base_inputs(&p, &[], &[], &[], &[]));
        let y2033 = out.annual.iter().find(|r| r.year == 2033).unwrap();
        let medicare = y2033
            .milestones
            .iter()
            .find(|m| m.label == "Medicare eligibility")
            .unwrap();
        assert!(medicare.detail.contains("your spouse"));
    }

    #[test]
    fn no_growth_no_flows_leaves_balances_flat() {
        let p = profile(1960, 2027 - 1960); // ends 2027 -> two years
        let accts = [account("a", "taxable", 100_000.0, 0.0)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &[], &[]));
        assert_eq!(out.summary.current_net_worth, 100_000.0);
        assert_eq!(out.summary.projected_ending_balance, 100_000.0);
        assert_eq!(out.summary.depletion_year, None);
    }

    #[test]
    fn growth_compounds_each_year() {
        let p = profile(1960, 2028 - 1960); // 2026, 2027, 2028 -> 3 years
        let accts = [account("a", "taxable", 100_000.0, 10.0)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &[], &[]));
        // 100k * 1.1^3 = 133,100
        assert!((out.summary.projected_ending_balance - 133_100.0).abs() < 1.0);
        assert_eq!(out.annual[0].growth, 10_000.0);
    }

    #[test]
    fn shortfall_draws_taxable_before_tax_deferred_before_tax_free() {
        let p = profile(1960, 2026 - 1960); // single year 2026
        let accts = [
            account("roth", "tax_free", 50_000.0, 0.0),
            account("ira", "tax_deferred", 50_000.0, 0.0),
            account("brok", "taxable", 50_000.0, 0.0),
        ];
        // Need 60k this year with no income.
        let spend = [spending("living", "essential", 60_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert_eq!(y.withdrawals, 60_000.0);
        // Taxable fully drained (50k), 10k pulled from tax-deferred, Roth untouched.
        let taxable_wd: f64 = out
            .quarterly
            .iter()
            .flat_map(|q| &q.withdrawals)
            .filter(|w| w.category == "taxable")
            .map(|w| w.amount)
            .sum();
        // Summed across all four quarters, the taxable draw is the full 50k.
        assert!((taxable_wd - 50_000.0).abs() < 1.0);
        // Roth should not appear in the first-year withdrawals at all.
        let roth_present = out
            .quarterly
            .iter()
            .flat_map(|q| &q.withdrawals)
            .any(|w| w.category == "tax_free");
        assert!(!roth_present);
    }

    #[test]
    fn depletion_year_is_flagged_when_accounts_run_dry() {
        let p = profile(1960, 2030 - 1960);
        let accts = [account("a", "taxable", 30_000.0, 0.0)];
        // 20k/yr spend, 30k saved -> runs out in the second year (2027).
        let spend = [spending("living", "essential", 20_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert_eq!(out.summary.depletion_year, Some(2027));
        assert!(out.annual[1].shortfall > 0.0);
    }

    #[test]
    fn surplus_income_is_reinvested() {
        let p = profile(1960, 2026 - 1960); // single year
        let accts = [account("a", "taxable", 10_000.0, 0.0)];
        // Tax-free income isolates the reinvestment logic from the tax engine.
        let mut inc = [income("pension", 40_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        inc[0].taxability = "tax_free".into();
        let spend = [spending("living", "essential", 30_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &inc, &spend, &[]));
        // 10k surplus reinvested onto the 10k balance.
        assert_eq!(out.annual[0].contributions, 10_000.0);
        assert_eq!(out.summary.projected_ending_balance, 20_000.0);
    }

    #[test]
    fn income_cola_grows_over_time() {
        let p = profile(1960, 2027 - 1960); // 2026, 2027
        let inc = [income("ss", 10_000.0, "annual", date(2026, 1, 1), true, 0.0)];
        let mut inputs = base_inputs(&p, &[], &inc, &[], &[]);
        inputs.social_security_cola_rate = 10.0;
        let out = run_projection(&inputs);
        assert_eq!(out.annual[0].income, 10_000.0);
        assert!((out.annual[1].income - 11_000.0).abs() < 1.0);
    }

    #[test]
    fn quarterly_schedule_has_four_quarters_summing_to_year() {
        let p = profile(1960, 2026 - 1960);
        let accts = [account("a", "taxable", 100_000.0, 0.0)];
        let spend = [spending("living", "essential", 40_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert_eq!(out.quarterly.len(), 4);
        let total: f64 = out.quarterly.iter().map(|q| q.total_withdrawal).sum();
        assert!((total - 40_000.0).abs() < 1.0);
        assert_eq!(out.quarterly[0].label, "2026 Q1");
    }

    #[test]
    fn healthcare_uses_healthcare_inflation() {
        let p = profile(1960, 2027 - 1960); // 2026, 2027
        let spend = [spending("meds", "healthcare", 10_000.0, "annual", true)];
        let mut inputs = base_inputs(&p, &[], &[], &spend, &[]);
        inputs.inflation_rate = 2.0;
        inputs.healthcare_inflation_rate = 10.0;
        let out = run_projection(&inputs);
        // Year 2 healthcare grows at 10%, not 2%.
        assert!((out.annual[1].spending - 11_000.0).abs() < 1.0);
    }

    // ---- Phase 2: tax integration -------------------------------------------

    #[test]
    fn tax_deferred_withdrawal_is_taxed_and_grosses_up_the_draw() {
        // Single filer, 80k spending funded entirely from a traditional IRA.
        // The withdrawal is ordinary income, so the draw must gross up to also
        // cover the resulting tax.
        let p = profile(1960, 2026 - 1960);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert!(y.taxes > 0.0, "expected tax on the IRA withdrawal");
        // Withdrawal covers spending plus tax.
        assert!(y.withdrawals > 80_000.0);
        assert!((y.withdrawals - 80_000.0 - y.taxes).abs() < 5.0);
        assert!((y.tax.ordinary_income - y.withdrawals).abs() < 5.0);
    }

    #[test]
    fn early_ira_withdrawal_before_fifty_nine_and_a_half_incurs_the_ten_percent_penalty() {
        // Born 1970-06-01: 56 in the single projection year (2026), well
        // short of the 59½ milestone (2029-12). All ordinary income comes
        // from the IRA, so the penalty should be ~10% of that draw.
        let p = profile(1970, 2026 - 1970);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert!(
            y.tax.early_withdrawal_penalty > 0.0,
            "expected a 10% early-withdrawal penalty on the pre-59½ IRA draw"
        );
        assert!(
            (y.tax.early_withdrawal_penalty - 0.10 * y.tax.ordinary_income).abs() < 5.0,
            "penalty {} should be ~10% of ordinary income {}",
            y.tax.early_withdrawal_penalty,
            y.tax.ordinary_income
        );
        // The penalty is included in the totals the rest of the app reads.
        assert!(y.tax.total_tax >= y.tax.early_withdrawal_penalty);
        assert!((y.tax.total_tax - y.taxes).abs() < 0.01);
    }

    #[test]
    fn ira_withdrawal_after_fifty_nine_and_a_half_has_no_penalty() {
        // Born 1966-06-01: 60 in the single projection year (2026), past the
        // 59½ milestone (2025-12).
        let p = profile(1966, 2026 - 1966);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert_eq!(y.tax.early_withdrawal_penalty, 0.0);
    }

    #[test]
    fn avoid_early_withdrawal_penalty_blocks_ira_draws_and_shortfalls_instead() {
        // Same pre-59½ setup as the penalty test above, but with the "Don't
        // incur penalties" flag on: the IRA must not be touched at all, so
        // spending goes entirely unfunded rather than drawing from it.
        let p = profile(1970, 2026 - 1970);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let inputs = ProjectionInputs {
            avoid_early_withdrawal_penalty: true,
            ..base_inputs(&p, &accts, &[], &spend, &[])
        };
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert_eq!(y.withdrawals, 0.0, "the IRA must not be drawn from before 59½");
        assert_eq!(y.tax.early_withdrawal_penalty, 0.0);
        assert!(
            (y.shortfall - 80_000.0).abs() < 1.0,
            "unfunded spending should show up as a shortfall instead"
        );
    }

    #[test]
    fn avoid_early_withdrawal_penalty_still_draws_a_taxable_account_first() {
        // A small taxable account plus a large pre-59½ IRA: with the flag on,
        // the taxable account is drawn down as usual, but the IRA is left
        // untouched even though it could otherwise cover the rest of spending.
        let p = profile(1970, 2026 - 1970);
        let accts = [
            account("brokerage", "taxable", 20_000.0, 0.0),
            account("ira", "tax_deferred", 500_000.0, 0.0),
        ];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let inputs = ProjectionInputs {
            avoid_early_withdrawal_penalty: true,
            ..base_inputs(&p, &accts, &[], &spend, &[])
        };
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert!((y.withdrawals - 20_000.0).abs() < 1.0);
        assert_eq!(y.tax.early_withdrawal_penalty, 0.0);
        assert!(y.shortfall > 0.0, "the IRA must not fill the remaining gap");
        let ira_balance = y
            .account_balances
            .iter()
            .find(|a| a.account_id == "ira")
            .unwrap();
        assert_eq!(ira_balance.change, 0.0, "the IRA balance must be untouched");
    }

    #[test]
    fn roth_withdrawals_are_tax_free() {
        let p = profile(1960, 2026 - 1960);
        let accts = [account("roth", "tax_free", 300_000.0, 0.0)];
        let spend = [spending("living", "essential", 40_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert_eq!(y.taxes, 0.0);
        assert!((y.withdrawals - 40_000.0).abs() < 1.0);
    }

    #[test]
    fn capital_gains_are_realized_on_taxable_withdrawals() {
        // Taxable account with an 80% embedded gain (basis 100k of 500k). A 50k
        // draw realizes 80% * 50k = 40k of long-term gains. With no other
        // income the gains sit in the 0% bracket, so no tax is owed.
        let p = profile(1960, 2026 - 1960);
        let mut a = account("brok", "taxable", 500_000.0, 0.0);
        a.cost_basis = Some(100_000.0);
        let accts = [a];
        let spend = [spending("living", "essential", 50_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert!((y.tax.capital_gains - 40_000.0).abs() < 1.0);
        assert_eq!(y.tax.federal_capital_gains_tax, 0.0); // 0% LTCG bracket
    }

    #[test]
    fn qualified_dividends_are_taxed_at_preferential_rates() {
        // 800k taxable account yielding 5% -> 40k qualified dividends, plus a
        // 60k taxable pension. Dividends stack on top of ordinary income.
        // Born 1965 (age 61) so no age-65 standard deduction bonus applies.
        let p = profile(1965, 2026 - 1965);
        let mut a = account("brok", "taxable", 800_000.0, 5.0);
        a.dividend_yield = 5.0;
        let accts = [a];
        let inc = [income("pension", 60_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let out = run_projection(&base_inputs(&p, &accts, &inc, &[], &[]));
        let y = &out.annual[0];
        assert!((y.tax.qualified_dividends - 40_000.0).abs() < 1.0);
        // Ordinary taxable 45k; 3,350 of dividends at 0% then 36,650 at 15%.
        assert!((y.tax.federal_capital_gains_tax - 5_497.5).abs() < 5.0);
    }

    #[test]
    fn social_security_is_partially_taxed_in_the_projection() {
        // 40k Social Security plus IRA draws pushes provisional income into the
        // taxable range without reaching the 85% cap.
        let p = profile(1955, 2026 - 1955);
        let ss = income_typed("ss", "social_security", 40_000.0, date(2020, 1, 1));
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 60_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[ss], &spend, &[]));
        let y = &out.annual[0];
        assert!(y.tax.taxable_social_security > 0.0);
        assert!(y.tax.taxable_social_security < 0.85 * 40_000.0);
        assert_eq!(y.tax.social_security_benefits, 40_000.0);
    }

    #[test]
    fn lifetime_tax_totals_accumulate_and_split() {
        let p = profile(1960, 2028 - 1960); // three years
        let accts = [account("ira", "tax_deferred", 1_000_000.0, 3.0)];
        let spend = [spending("living", "essential", 90_000.0, "annual", false)];
        let mut inputs = base_inputs(&p, &accts, &[], &spend, &[]);
        inputs.inflation_rate = 2.0;
        let out = run_projection(&inputs);
        let sum_annual: f64 = out.annual.iter().map(|y| y.taxes).sum();
        assert!((out.summary.total_lifetime_taxes - sum_annual).abs() < 1.0);
        assert!(
            (out.summary.total_lifetime_taxes
                - out.summary.total_lifetime_federal_taxes
                - out.summary.total_lifetime_state_taxes)
                .abs()
                < 1.0
        );
        assert!(out.summary.total_lifetime_taxes > 0.0);
    }

    #[test]
    fn no_income_tax_states_add_no_state_tax() {
        let mut p = profile(1960, 2026 - 1960);
        p.state = "TX".into();
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 90_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert_eq!(out.annual[0].tax.state_tax, 0.0);

        // California, same plan, owes state tax.
        p.state = "CA".into();
        let out_ca = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert!(out_ca.annual[0].tax.state_tax > 0.0);
    }

    // ---- Phase 2, feature 6: Roth conversions -------------------------------

    #[test]
    fn roth_conversion_fills_up_to_the_ceiling() {
        // Cash to pay the conversion tax (so the IRA isn't tapped and grossed
        // up), a large traditional IRA, and a Roth to receive the conversion.
        // Born 1960 -> age 66 in 2026, so the standard deduction is 17,000
        // (15,000 + 2,000 senior). A 50k taxable-income ceiling therefore
        // targets 67k of AGI, i.e. a 67k conversion, landing taxable income at
        // exactly the 50k ceiling.
        let p = profile(1960, 2026 - 1960);
        let mut cash = account("cash", "taxable", 100_000.0, 0.0);
        cash.cost_basis = Some(100_000.0); // no embedded gain
        let accts = [
            cash,
            account("ira", "tax_deferred", 500_000.0, 0.0),
            account("roth", "tax_free", 10_000.0, 0.0),
        ];
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_ceiling = 50_000.0;
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert!((y.roth_conversion - 67_000.0).abs() < 50.0);
        assert!((y.tax.taxable_income - 50_000.0).abs() < 50.0);
        // The conversion is ordinary income and is taxed.
        assert!(y.taxes > 0.0);
        assert!((y.tax.ordinary_income - y.roth_conversion).abs() < 1.0);
    }

    #[test]
    fn roth_conversion_moves_balance_from_traditional_to_roth() {
        let p = profile(1960, 2026 - 1960);
        let accts = [
            account("ira", "tax_deferred", 500_000.0, 0.0),
            account("roth", "tax_free", 0.0, 0.0),
        ];
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_ceiling = 50_000.0;
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        // Traditional shrinks by the conversion; net worth falls only by the tax
        // paid (funded from the taxable/cash pool — here from the Roth is last,
        // so the tax comes out of the traditional draw itself).
        assert!(y.roth_conversion > 0.0);
        assert_eq!(
            round2(out.summary.total_lifetime_roth_conversions),
            round2(y.roth_conversion),
        );
        // Ending net worth = starting − taxes paid.
        assert!((y.starting_balance - y.taxes - y.ending_balance).abs() < 5.0);
    }

    #[test]
    fn roth_conversion_respects_year_window() {
        let p = profile(1960, 2028 - 1960); // 2026, 2027, 2028
        let accts = [
            account("ira", "tax_deferred", 500_000.0, 0.0),
            account("roth", "tax_free", 0.0, 0.0),
        ];
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_ceiling = 40_000.0;
        inputs.roth_conversion_start_year = Some(2027);
        inputs.roth_conversion_end_year = Some(2027);
        let out = run_projection(&inputs);
        assert_eq!(out.annual[0].roth_conversion, 0.0); // 2026 out of window
        assert!(out.annual[1].roth_conversion > 0.0); // 2027 converts
        assert_eq!(out.annual[2].roth_conversion, 0.0); // 2028 out of window
    }

    #[test]
    fn roth_conversion_skipped_without_a_roth_account() {
        // A ceiling is set but there is no tax-free destination: no conversion.
        let p = profile(1960, 2026 - 1960);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_ceiling = 50_000.0;
        let out = run_projection(&inputs);
        assert_eq!(out.annual[0].roth_conversion, 0.0);
        assert_eq!(out.annual[0].taxes, 0.0);
    }

    // ---- Phase 3, feature 6: IRMAA/RMD-aware Roth conversion timing --------

    /// Cash to pay the conversion tax (so the IRA isn't tapped and grossed
    /// up beyond the planned conversion itself), a large traditional IRA, and
    /// a Roth to receive the conversion — same shape as
    /// `roth_conversion_fills_up_to_the_ceiling`.
    fn irmaa_aware_accounts() -> [Account; 3] {
        let mut cash = account("cash", "taxable", 200_000.0, 0.0);
        cash.cost_basis = Some(200_000.0); // no embedded gain
        [
            cash,
            account("ira", "tax_deferred", 2_000_000.0, 0.0),
            account("roth", "tax_free", 0.0, 0.0),
        ]
    }

    #[test]
    fn irmaa_aware_roth_conversion_fills_up_to_the_next_irmaa_tier() {
        // No other income and below RMD age: the whole conversion room comes
        // from the gap between $0 MAGI and the first single IRMAA tier
        // ($106,000, with 0% inflation so the base-year threshold applies
        // as-is), minus the $1 avoidance buffer.
        let p = profile(1965, 2026 - 1965); // 61 in 2026, RMD age is 75
        let accts = irmaa_aware_accounts();
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_mode = "irmaa_aware".to_string();
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert!((y.roth_conversion - 105_999.0).abs() < 50.0);
        assert!(y.tax.magi < 106_000.0);
    }

    #[test]
    fn irmaa_aware_roth_conversion_shrinks_as_rmd_grows() {
        // Same $2M IRA and IRMAA tier target as above, but the household is
        // now well past RMD age. The forced RMD already counts toward the
        // tier, so the voluntary conversion should shrink to whatever room
        // the RMD hasn't already used — landing the *combined* ordinary
        // income (RMD + conversion) at roughly the same tier target as the
        // no-RMD case, rather than stacking on top of it.
        let below_rmd_age = {
            let p = profile(1965, 2026 - 1965); // 61 in 2026
            let accts = irmaa_aware_accounts();
            let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
            inputs.roth_conversion_mode = "irmaa_aware".to_string();
            run_projection(&inputs).annual[0].roth_conversion
        };
        let p = profile(1950, 2026 - 1950); // 76 in 2026, well past RMD age 72
        let accts = irmaa_aware_accounts();
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_mode = "irmaa_aware".to_string();
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert!(y.rmd_amount > 0.0);
        assert!(
            y.roth_conversion < below_rmd_age,
            "the RMD should crowd out most of the discretionary conversion"
        );
        assert!(
            (y.rmd_amount + y.roth_conversion - 105_999.0).abs() < 100.0,
            "RMD + conversion should still target the same IRMAA tier, RMD={}, conversion={}",
            y.rmd_amount,
            y.roth_conversion
        );
    }

    #[test]
    fn irmaa_aware_roth_conversion_respects_a_flat_ceiling_as_an_extra_cap() {
        // Same setup as `roth_conversion_fills_up_to_the_ceiling`, but in
        // IRMAA-aware mode: the $50k taxable-income ceiling (67k AGI, given
        // the 17k standard deduction at age 66) is the binding constraint
        // here since it's tighter than the ~106k IRMAA-tier room.
        let p = profile(1960, 2026 - 1960);
        let accts = irmaa_aware_accounts();
        let mut inputs = base_inputs(&p, &accts, &[], &[], &[]);
        inputs.roth_conversion_mode = "irmaa_aware".to_string();
        inputs.roth_conversion_ceiling = 50_000.0;
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert!((y.roth_conversion - 67_000.0).abs() < 50.0);
    }

    // ---- Phase 2, feature 7: estimated quarterly taxes ----------------------

    #[test]
    fn estimated_taxes_split_into_four_dated_installments() {
        let p = profile(1960, 2026 - 1960);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let est = &out.estimated_taxes;
        assert_eq!(est.tax_year, 2026);
        assert_eq!(est.payments.len(), 4);
        // Installments reconcile to the year's total.
        let sum: f64 = est.payments.iter().map(|p| p.amount).sum();
        assert!((sum - est.total).abs() < 0.05);
        assert!((est.total - out.annual[0].taxes).abs() < 0.05);
        // Standard IRS 1040-ES due dates, with the last landing in the new year.
        assert_eq!(est.payments[0].due_date, "2026-04-15");
        assert_eq!(est.payments[1].due_date, "2026-06-15");
        assert_eq!(est.payments[2].due_date, "2026-09-15");
        assert_eq!(est.payments[3].due_date, "2027-01-15");
    }

    #[test]
    fn estimated_taxes_are_zero_when_no_tax_is_owed() {
        let p = profile(1960, 2026 - 1960);
        let accts = [account("roth", "tax_free", 300_000.0, 0.0)];
        let spend = [spending("living", "essential", 40_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert_eq!(out.estimated_taxes.total, 0.0);
        assert!(out.estimated_taxes.payments.iter().all(|p| p.amount == 0.0));
    }

    // ---- Phase 2, feature 9: withdrawal sequencing optimization -------------

    /// Sum of a first-year account's withdrawals across all four quarters.
    fn account_withdrawal(out: &ProjectionResponse, account_id: &str) -> f64 {
        out.quarterly
            .iter()
            .flat_map(|q| &q.withdrawals)
            .filter(|w| w.account_id == account_id)
            .map(|w| w.amount)
            .sum()
    }

    #[test]
    fn conventional_strategy_draws_largest_taxable_balance_first() {
        // Two taxable accounts, no embedded-gain difference driving the choice
        // — conventional always prefers the larger balance. "big" (150k) should
        // be drawn before "small" (50k) to cover a 100k need.
        let p = profile(1960, 2026 - 1960);
        let accts = [
            account("small", "taxable", 50_000.0, 0.0),
            account("big", "taxable", 150_000.0, 0.0),
        ];
        let spend = [spending("living", "essential", 100_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert_eq!(account_withdrawal(&out, "big"), 100_000.0);
        assert_eq!(account_withdrawal(&out, "small"), 0.0);
    }

    #[test]
    fn tax_optimized_realizes_the_cheapest_gains_first() {
        // Two taxable accounts of different sizes and embedded gains: "cheap"
        // (100k balance, 10% gain) is smaller than "pricey" (200k balance, 90%
        // gain), so conventional's largest-balance-first rule draws "pricey"
        // first and realizes a big gain. The optimized strategy should prefer
        // "cheap" instead, even though it's the smaller account.
        let p = profile(1960, 2026 - 1960);
        let mut cheap = account("cheap", "taxable", 100_000.0, 0.0);
        cheap.cost_basis = Some(90_000.0); // 10% embedded gain
        let mut pricey = account("pricey", "taxable", 200_000.0, 0.0);
        pricey.cost_basis = Some(20_000.0); // 90% embedded gain
        let accts = [cheap, pricey];
        let spend = [spending("living", "essential", 100_000.0, "annual", false)];

        let mut conventional = base_inputs(&p, &accts, &[], &spend, &[]);
        conventional.withdrawal_strategy = "conventional".to_string();
        let out_conv = run_projection(&conventional);
        // Conventional drains the larger ("pricey") account first, realizing
        // ~90% of the draw as gain (the draw grosses up above 100k to also
        // cover the resulting capital-gains tax, since most of it isn't
        // shielded by the standard deduction).
        let conv_draw = account_withdrawal(&out_conv, "pricey");
        assert!(conv_draw > 100_000.0);
        assert_eq!(account_withdrawal(&out_conv, "cheap"), 0.0);
        assert!((out_conv.annual[0].tax.capital_gains - conv_draw * 0.9).abs() < 1.0);

        let mut optimized = base_inputs(&p, &accts, &[], &spend, &[]);
        optimized.withdrawal_strategy = "tax_optimized".to_string();
        let out_opt = run_projection(&optimized);
        // Tax-optimized drains the lower-gain ("cheap") account fully first
        // (need exactly matches its 100k balance), realizing only 10k of gain.
        assert_eq!(account_withdrawal(&out_opt, "cheap"), 100_000.0);
        assert_eq!(account_withdrawal(&out_opt, "pricey"), 0.0);
        assert!((out_opt.annual[0].tax.capital_gains - 10_000.0).abs() < 1.0);

        // Realizing far less gain means materially less tax overall.
        assert!(out_opt.annual[0].taxes < out_conv.annual[0].taxes);
    }

    #[test]
    fn tax_optimized_draws_tax_deferred_before_a_pricier_taxable_gain() {
        // Born 1965 (age 61 in 2026) so no age-65 standard-deduction bonus
        // applies. Pension income (63,400, single, std deduction 15,000) puts
        // baseline ordinary taxable income at exactly 48,400 — inside the
        // narrow band where the federal ordinary bracket is still 12% (up to
        // 48,475) but the capital-gains bracket has already stepped up to 15%
        // (from 48,350). With a taxable account that is 95% embedded gain,
        // realizing its gains (15% * 0.95 = 14.25%) costs more at the margin
        // than an equivalent ordinary tax-deferred withdrawal (12%), so the
        // optimized strategy should swap the category order for this year.
        let p = profile(1965, 2026 - 1965);
        let mut taxable = account("taxable_acct", "taxable", 200_000.0, 0.0);
        taxable.cost_basis = Some(10_000.0); // 95% embedded gain
        let deferred = account("deferred_acct", "tax_deferred", 200_000.0, 0.0);
        let accts = [taxable, deferred];
        let inc = [income("pension", 63_400.0, "annual", date(2020, 1, 1), false, 0.0)];
        let spend = [spending("living", "essential", 100_000.0, "annual", false)];

        let mut conventional = base_inputs(&p, &accts, &inc, &spend, &[]);
        conventional.withdrawal_strategy = "conventional".to_string();
        let out_conv = run_projection(&conventional);
        assert_eq!(out_conv.annual[0].withdrawal_order, "taxable_first");
        assert!(account_withdrawal(&out_conv, "taxable_acct") > 0.0);

        let mut optimized = base_inputs(&p, &accts, &inc, &spend, &[]);
        optimized.withdrawal_strategy = "tax_optimized".to_string();
        let out_opt = run_projection(&optimized);
        assert_eq!(out_opt.annual[0].withdrawal_order, "tax_deferred_first");
        // The swap draws tax-deferred funds before touching the (pricier)
        // taxable gains, so tax-deferred takes on more of the load than under
        // the conventional order (which draws taxable first).
        assert!(
            account_withdrawal(&out_opt, "deferred_acct")
                > account_withdrawal(&out_conv, "deferred_acct")
        );
        assert!(
            account_withdrawal(&out_opt, "taxable_acct")
                < account_withdrawal(&out_conv, "taxable_acct")
        );
    }

    // ---- Phase 2, feature 9b: maximize-income (bracket-fill) strategy -----

    /// A large IRA relative to spending, with several low-income years before
    /// RMDs begin (born 1958 -> RMD age 73 -> RMDs start 2031) and a long tail
    /// afterward: with only modest spending drawn against a $3M balance, the
    /// IRS divisor keeps shrinking every year long after the balance itself
    /// has stopped shrinking much, so RMDs eventually dwarf the spending need
    /// and land in a higher bracket than the modest pre-RMD withdrawals ever
    /// would. A small taxable account gives the strategy somewhere to
    /// reinvest voluntary withdrawals beyond need.
    fn maximize_income_scenario() -> (Profile, [Account; 2], [SpendingItem; 1]) {
        let p = profile(1958, 105); // ends 2063 (1958 + 105)
        let accts = [
            account("ira", "tax_deferred", 3_000_000.0, 0.0),
            account("brokerage", "taxable", 10_000.0, 0.0),
        ];
        let spend = [spending("living", "essential", 10_000.0, "annual", false)];
        (p, accts, spend)
    }

    #[test]
    fn maximize_income_draws_more_from_the_ira_than_tax_optimized_in_a_pre_rmd_year() {
        let (p, accts, spend) = maximize_income_scenario();

        let mut optimized = base_inputs(&p, &accts, &[], &spend, &[]);
        optimized.withdrawal_strategy = "tax_optimized".to_string();
        let out_opt = run_projection(&optimized);

        let mut maximized = base_inputs(&p, &accts, &[], &spend, &[]);
        maximized.withdrawal_strategy = "maximize_income".to_string();
        let out_max = run_projection(&maximized);

        // 2026 is five years before RMDs begin (2031): tax-optimized only
        // draws the IRA down enough to cover the 10k spending need (the
        // taxable brokerage account alone already covers that), while
        // maximize-income should voluntarily draw much more from the IRA to
        // fill a low bracket now, shrinking the balance ahead of the eventual
        // RMD.
        let opt_year0 = &out_opt.annual[0];
        let max_year0 = &out_max.annual[0];
        assert_eq!(opt_year0.year, 2026);
        assert_eq!(account_withdrawal(&out_opt, "ira"), 0.0);
        assert!(account_withdrawal(&out_max, "ira") > 0.0);
        assert!(max_year0.withdrawals > opt_year0.withdrawals);
        assert!(max_year0.ending_balance < opt_year0.ending_balance);
    }

    #[test]
    fn maximize_income_reduces_lifetime_rmds_and_taxes_versus_tax_optimized() {
        let (p, accts, spend) = maximize_income_scenario();

        let mut optimized = base_inputs(&p, &accts, &[], &spend, &[]);
        optimized.withdrawal_strategy = "tax_optimized".to_string();
        let out_opt = run_projection(&optimized);

        let mut maximized = base_inputs(&p, &accts, &[], &spend, &[]);
        maximized.withdrawal_strategy = "maximize_income".to_string();
        let out_max = run_projection(&maximized);

        // Shrinking the IRA before RMD age means smaller RMDs once they begin...
        assert!(out_max.summary.total_lifetime_rmd < out_opt.summary.total_lifetime_rmd);
        // ...which the search only keeps if it actually pays off in more total
        // lifetime after-tax cash than not accelerating at all.
        assert!(maximize_income_score(&out_max.summary) >= maximize_income_score(&out_opt.summary));
    }

    #[test]
    fn maximize_income_is_a_no_op_with_only_a_tax_deferred_account() {
        // With nowhere but the same tax-deferred account to reinvest a
        // voluntary withdrawal's after-tax proceeds, drawing more than
        // spending/RMDs require would only tax a dollar today for no benefit.
        // The search must recognize there's no valid destination and behave
        // identically to tax-optimized.
        let p = profile(1958, 105);
        let accts = [account("ira", "tax_deferred", 3_000_000.0, 0.0)];
        let spend = [spending("living", "essential", 10_000.0, "annual", false)];

        let mut optimized = base_inputs(&p, &accts, &[], &spend, &[]);
        optimized.withdrawal_strategy = "tax_optimized".to_string();
        let out_opt = run_projection(&optimized);

        let mut maximized = base_inputs(&p, &accts, &[], &spend, &[]);
        maximized.withdrawal_strategy = "maximize_income".to_string();
        let out_max = run_projection(&maximized);

        assert_eq!(out_max.summary.total_lifetime_taxes, out_opt.summary.total_lifetime_taxes);
        assert_eq!(out_max.summary.total_lifetime_rmd, out_opt.summary.total_lifetime_rmd);
        assert_eq!(out_max.annual[0].withdrawals, out_opt.annual[0].withdrawals);
    }

    #[test]
    fn maximize_income_reinvests_bracket_fill_proceeds_into_tax_free_not_tax_deferred() {
        // A Roth (tax-free), not a taxable account, is the only place other
        // than the IRA itself to hold a voluntary withdrawal. Reinvesting into
        // the IRA it came from would just tax the same dollar for nothing, so
        // the engine must route it to the Roth instead.
        let p = profile(1958, 105);
        let accts = [
            account("ira", "tax_deferred", 3_000_000.0, 0.0),
            account("roth", "tax_free", 10_000.0, 0.0),
        ];
        let spend = [spending("living", "essential", 10_000.0, "annual", false)];

        let mut maximized = base_inputs(&p, &accts, &[], &spend, &[]);
        maximized.withdrawal_strategy = "maximize_income".to_string();
        let out_max = run_projection(&maximized);

        let mut optimized = base_inputs(&p, &accts, &[], &spend, &[]);
        optimized.withdrawal_strategy = "tax_optimized".to_string();
        let out_opt = run_projection(&optimized);

        // The strategy found a genuinely better candidate than doing nothing...
        assert!(maximize_income_score(&out_max.summary) > maximize_income_score(&out_opt.summary));
        // ...and the extra money it pulled out of the IRA landed in the Roth,
        // not back in the IRA.
        assert!(account_withdrawal(&out_max, "ira") > account_withdrawal(&out_opt, "ira"));
        let roth_end = |out: &ProjectionResponse| {
            out.annual[0]
                .account_balances
                .iter()
                .find(|a| a.account_name == "acct-roth")
                .unwrap()
                .ending_balance
        };
        assert!(roth_end(&out_max) > roth_end(&out_opt));
    }

    // ---- RMD module ----------------------------------------------------

    #[test]
    fn rmd_is_zero_before_the_required_beginning_age() {
        let p = profile(1960, 2026 - 1960); // age 66 in 2026; RMD age is 75
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &[], &[]));
        assert_eq!(out.annual[0].rmd_amount, 0.0);
    }

    #[test]
    fn rmd_forces_a_minimum_withdrawal_even_without_spending_need() {
        // Born 1950 -> RMD age 72; by 2026 (age 76) RMDs are already due.
        let p = profile(1950, 2026 - 1950); // single year, age 76
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        // No spending and no other income: nothing is *needed*, but the RMD
        // floor should still force a draw.
        let out = run_projection(&base_inputs(&p, &accts, &[], &[], &[]));
        let y = &out.annual[0];
        // 500k / 23.7 (age-76 Uniform Lifetime divisor) ≈ 21,097.05.
        assert!((y.rmd_amount - 21_097.05).abs() < 5.0);
        assert!(y.withdrawals >= y.rmd_amount - 1.0);
        assert!(y.taxes > 0.0, "the forced distribution is taxable income");
        // Not needed for spending, so the after-tax proceeds are reinvested.
        assert!(y.contributions > 0.0);
        assert!((y.starting_balance + y.growth - y.withdrawals + y.contributions
            - y.ending_balance)
            .abs()
            < 1.0);
        assert!((out.summary.total_lifetime_rmd - y.rmd_amount).abs() < 1.0);
    }

    #[test]
    fn rmd_is_based_on_prior_year_end_balance_not_this_years_growth() {
        // A big return this year should not inflate this year's own RMD.
        let p = profile(1950, 2026 - 1950);
        let accts = [account("ira", "tax_deferred", 500_000.0, 20.0)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &[], &[]));
        assert!((out.annual[0].rmd_amount - 21_097.05).abs() < 5.0);
    }

    #[test]
    fn rmd_only_counts_the_balance_of_accounts_owned_by_the_person_who_is_of_age() {
        // Primary (born 1950, RMD age 72) is 76 in 2026; spouse (born 1980)
        // is nowhere near RMD age. Only the primary's IRA should count.
        let mut p = profile(1950, 2026 - 1950);
        p.marital_status = "married".into();
        p.filing_status = "married_filing_jointly".into();
        p.spouse_date_of_birth = Some(date(1980, 6, 1));
        p.spouse_life_expectancy = Some(90);
        let mut spouse_ira = account("spouse_ira", "tax_deferred", 500_000.0, 0.0);
        spouse_ira.owner = "spouse".into();
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0), spouse_ira];
        let out = run_projection(&base_inputs(&p, &accts, &[], &[], &[]));
        assert!((out.annual[0].rmd_amount - 21_097.05).abs() < 5.0);
    }

    #[test]
    fn rmd_already_covered_by_spending_driven_withdrawals_adds_no_extra() {
        // Spending alone already exceeds the RMD, so the floor is a no-op:
        // withdrawals should match the un-floored (spending-only) amount.
        let p = profile(1950, 2026 - 1950);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        let y = &out.annual[0];
        assert!(y.withdrawals > y.rmd_amount);
        assert!((y.withdrawals - 80_000.0 - y.taxes).abs() < 5.0);
    }

    #[test]
    fn withdrawal_order_defaults_to_taxable_first() {
        // The default (conventional) strategy always labels the order
        // "taxable_first", even when tax-deferred funds are also drawn.
        let p = profile(1960, 2026 - 1960);
        let accts = [account("ira", "tax_deferred", 500_000.0, 0.0)];
        let spend = [spending("living", "essential", 80_000.0, "annual", false)];
        let out = run_projection(&base_inputs(&p, &accts, &[], &spend, &[]));
        assert_eq!(out.annual[0].withdrawal_order, "taxable_first");
    }

    // ---- Phase 3, feature 1: ACA subsidy ------------------------------------

    #[test]
    fn aca_subsidy_offsets_spending_before_medicare() {
        // Single filer age 61 with $30k taxable pension (~192% FPL). A $12k
        // benchmark premium yields a large premium tax credit that offsets
        // spending, so far less has to be withdrawn than without it.
        let p = profile(1965, 2026 - 1965);
        let mut cash = account("cash", "taxable", 100_000.0, 0.0);
        cash.cost_basis = Some(100_000.0);
        let accts = [cash];
        let inc = [income("pension", 30_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let spend = [spending("living", "essential", 40_000.0, "annual", false)];

        let mut with_aca = base_inputs(&p, &accts, &inc, &spend, &[]);
        with_aca.aca_benchmark_annual_premium = 12_000.0;
        let out = run_projection(&with_aca);
        let y = &out.annual[0];
        assert!(y.aca.eligible);
        assert!((y.aca.fpl_percent - 191.7).abs() < 1.0);
        assert!((y.aca.subsidy - 11_500.0).abs() < 50.0);
        assert!((out.summary.total_lifetime_aca_subsidies - y.aca.subsidy).abs() < 1.0);

        // Without the benchmark, the same plan withdraws far more to cover the
        // spending gap the subsidy would otherwise fill.
        let out_none = run_projection(&base_inputs(&p, &accts, &inc, &spend, &[]));
        assert_eq!(out_none.annual[0].aca.subsidy, 0.0);
        assert!(out_none.annual[0].withdrawals > y.withdrawals + 10_000.0);
    }

    #[test]
    fn aca_subsidy_stops_at_medicare_age() {
        // Age 68: past Medicare eligibility, so no ACA subsidy even with a
        // benchmark premium set.
        let p = profile(1958, 2026 - 1958);
        let inc = [income("pension", 30_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let mut inputs = base_inputs(&p, &[], &inc, &[], &[]);
        inputs.aca_benchmark_annual_premium = 12_000.0;
        let out = run_projection(&inputs);
        assert!(!out.annual[0].aca.eligible);
        assert_eq!(out.annual[0].aca.subsidy, 0.0);
    }

    #[test]
    fn aca_ineligible_below_the_poverty_line() {
        // Age 61 but income (Roth-funded, so ~$0 MAGI) is under 100% FPL:
        // Medicaid territory, no premium tax credit.
        let p = profile(1965, 2026 - 1965);
        let accts = [account("roth", "tax_free", 300_000.0, 0.0)];
        let spend = [spending("living", "essential", 40_000.0, "annual", false)];
        let mut inputs = base_inputs(&p, &accts, &[], &spend, &[]);
        inputs.aca_benchmark_annual_premium = 12_000.0;
        let out = run_projection(&inputs);
        assert!(out.annual[0].aca.fpl_percent < 100.0);
        assert!(!out.annual[0].aca.eligible);
        assert_eq!(out.annual[0].aca.subsidy, 0.0);
    }

    #[test]
    fn roth_conversion_shrinks_the_aca_subsidy() {
        // The core ACA/Roth tradeoff: converting to Roth raises MAGI, which
        // pushes the household up the FPL scale and shrinks the subsidy.
        let p = profile(1962, 2026 - 1962); // age 64, still pre-Medicare
        let mut cash = account("cash", "taxable", 100_000.0, 0.0);
        cash.cost_basis = Some(100_000.0);
        let accts = [
            cash,
            account("ira", "tax_deferred", 500_000.0, 0.0),
            account("roth", "tax_free", 10_000.0, 0.0),
        ];
        let inc = [income("pension", 25_000.0, "annual", date(2020, 1, 1), false, 0.0)];

        let mut without = base_inputs(&p, &accts, &inc, &[], &[]);
        without.aca_benchmark_annual_premium = 15_000.0;
        let base = run_projection(&without);

        let mut with_conv = base_inputs(&p, &accts, &inc, &[], &[]);
        with_conv.aca_benchmark_annual_premium = 15_000.0;
        with_conv.roth_conversion_ceiling = 50_000.0;
        let conv = run_projection(&with_conv);

        assert!(conv.annual[0].roth_conversion > 0.0);
        assert!(conv.annual[0].aca.magi > base.annual[0].aca.magi);
        assert!(conv.annual[0].aca.subsidy < base.annual[0].aca.subsidy);
        // Both still receive some subsidy (both under the benchmark).
        assert!(base.annual[0].aca.subsidy > 0.0);
        assert!(conv.annual[0].aca.subsidy > 0.0);
    }

    // ---- Phase 3, feature 2: MAGI tracking ----------------------------------

    #[test]
    fn magi_is_tracked_every_year_even_without_aca() {
        // No ACA benchmark set, but MAGI should still be computed every year
        // (AGI plus the untaxed portion of Social Security) so it can be
        // forecast independently of ACA eligibility.
        let p = profile(1960, 2026 - 1960);
        let inc = [
            income("pension", 40_000.0, "annual", date(2020, 1, 1), false, 0.0),
            income_typed("ss", "social_security", 20_000.0, date(2020, 1, 1)),
        ];
        let out = run_projection(&base_inputs(&p, &[], &inc, &[], &[]));
        let y = &out.annual[0];
        assert_eq!(y.aca.subsidy, 0.0); // ACA modeling is off
        assert!(y.tax.magi > y.tax.adjusted_gross_income);
        let expected = y.tax.adjusted_gross_income
            + (y.tax.social_security_benefits - y.tax.taxable_social_security).max(0.0);
        assert!((y.tax.magi - expected).abs() < 1.0);
    }

    #[test]
    fn magi_matches_the_aca_magi_when_aca_is_active() {
        // When the ACA subsidy is being computed, the general-purpose MAGI
        // tracked on `tax` should agree with the MAGI the subsidy itself used.
        let p = profile(1965, 2026 - 1965);
        let inc = [income("pension", 30_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let mut inputs = base_inputs(&p, &[], &inc, &[], &[]);
        inputs.aca_benchmark_annual_premium = 12_000.0;
        let out = run_projection(&inputs);
        let y = &out.annual[0];
        assert!(y.aca.eligible);
        assert!((y.tax.magi - y.aca.magi).abs() < 1.0);
    }

    // ---- Phase 3, feature 3: Medicare Part B premium & enrollment ----------

    #[test]
    fn medicare_premium_is_zero_before_65_and_charged_from_65() {
        // life_expectancy is added directly to birth_year for the horizon, so
        // 65 extends the projection through 2027 (65 in 2027).
        let p = profile(1962, 65); // 64 in 2026, 65 in 2027
        let mut inputs = base_inputs(&p, &[], &[], &[], &[]);
        inputs.medicare_part_b_annual_premium = 2_220.0;
        let out = run_projection(&inputs);
        let y2026 = out.annual.iter().find(|r| r.year == 2026).unwrap();
        let y2027 = out.annual.iter().find(|r| r.year == 2027).unwrap();
        assert_eq!(y2026.medicare_premiums, 0.0);
        assert_eq!(y2027.medicare_premiums, 2_220.0);
        assert_eq!(out.summary.total_lifetime_medicare_premiums, 2_220.0);
    }

    #[test]
    fn medicare_premium_disabled_when_zero() {
        let p = profile(1955, 2026 - 1955); // already past 65
        let out = run_projection(&base_inputs(&p, &[], &[], &[], &[]));
        assert_eq!(out.annual[0].medicare_premiums, 0.0);
        assert_eq!(out.summary.total_lifetime_medicare_premiums, 0.0);
    }

    #[test]
    fn medicare_premium_charged_per_spouse_independently() {
        // Primary is 65 in 2026 (one premium); spouse doesn't turn 65 until
        // 2030, when the household starts paying two premiums.
        let mut p = profile(1961, 2050 - 1961);
        p.spouse_date_of_birth = Some(date(1965, 1, 1));
        p.spouse_life_expectancy = Some(90);
        let mut inputs = base_inputs(&p, &[], &[], &[], &[]);
        inputs.medicare_part_b_annual_premium = 2_000.0;
        let out = run_projection(&inputs);
        let y2026 = out.annual.iter().find(|r| r.year == 2026).unwrap();
        let y2030 = out.annual.iter().find(|r| r.year == 2030).unwrap();
        assert_eq!(y2026.medicare_premiums, 2_000.0);
        assert_eq!(y2030.medicare_premiums, 4_000.0);
    }

    #[test]
    fn medicare_premium_grows_with_healthcare_inflation() {
        let p = profile(1961, 2027 - 1961); // 65 in 2026
        let mut inputs = base_inputs(&p, &[], &[], &[], &[]);
        inputs.medicare_part_b_annual_premium = 2_000.0;
        inputs.healthcare_inflation_rate = 10.0;
        let out = run_projection(&inputs);
        let y2027 = out.annual.iter().find(|r| r.year == 2027).unwrap();
        // One year of 10% healthcare inflation on the age-65 premium.
        assert!((y2027.medicare_premiums - 2_200.0).abs() < 0.5);
    }

    #[test]
    fn medicare_enrollment_window_milestones_bracket_the_65th_birthday() {
        // Born June 1965: turns 65 in June 2030. The Initial Enrollment
        // Period opens 3 months before (March 2030, same year) and closes 3
        // months after (September 2030, same year here).
        let p = profile(1965, 2050 - 1965);
        let out = run_projection(&base_inputs(&p, &[], &[], &[], &[]));
        let labels = |year: i32| -> Vec<String> {
            out.annual
                .iter()
                .find(|r| r.year == year)
                .map(|r| r.milestones.iter().map(|m| m.label.clone()).collect())
                .unwrap_or_default()
        };
        assert!(labels(2030).contains(&"Medicare enrollment window opens".to_string()));
        assert!(labels(2030).contains(&"Medicare eligibility".to_string()));
        assert!(labels(2030).contains(&"Medicare enrollment window closes".to_string()));
    }

    #[test]
    fn delayed_medicare_enrollment_age_pushes_back_the_part_b_premium() {
        // Born 1961 -> 65 in 2026. With a delayed enrollment age of 70, the
        // premium shouldn't start until 2031.
        let p = profile(1961, 2032 - 1961);
        let mut inputs = base_inputs(&p, &[], &[], &[], &[]);
        inputs.medicare_part_b_annual_premium = 2_220.0;
        inputs.medicare_enrollment_age = 70;
        let out = run_projection(&inputs);
        let y2026 = out.annual.iter().find(|r| r.year == 2026).unwrap(); // 65
        let y2030 = out.annual.iter().find(|r| r.year == 2030).unwrap(); // 69
        let y2031 = out.annual.iter().find(|r| r.year == 2031).unwrap(); // 70
        assert_eq!(y2026.medicare_premiums, 0.0);
        assert_eq!(y2030.medicare_premiums, 0.0);
        assert_eq!(y2031.medicare_premiums, 2_220.0);
    }

    #[test]
    fn delayed_medicare_enrollment_age_also_delays_irmaa_enrollment_counting() {
        // Born 1959 -> 69 in 2028, 70 in 2029. High pension income sits well
        // inside an IRMAA tier once lookback data exists (from 2028, using
        // 2026's MAGI), but with enrollment delayed to 70 the household
        // shouldn't be counted as enrolled — and so shouldn't pay any
        // surcharge — until 2029.
        let p = profile(1959, 2032 - 1959);
        let inc = [income("pension", 150_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let mut inputs = base_inputs(&p, &[], &inc, &[], &[]);
        inputs.medicare_part_b_annual_premium = 2_220.0;
        inputs.medicare_enrollment_age = 70;
        let out = run_projection(&inputs);
        let y2028 = out.annual.iter().find(|r| r.year == 2028).unwrap(); // 69
        let y2029 = out.annual.iter().find(|r| r.year == 2029).unwrap(); // 70
        assert!(y2028.irmaa.has_lookback_data);
        assert_eq!(y2028.irmaa.enrolled_count, 0);
        assert_eq!(y2028.irmaa_surcharge, 0.0);
        assert_eq!(y2029.irmaa.enrolled_count, 1);
        assert!(y2029.irmaa_surcharge > 0.0);
    }

    // ---- Phase 3, feature 4: Medicare IRMAA surcharge -----------------------

    /// A single filer already well past 65 with a flat, high taxable pension —
    /// comfortably inside the second single IRMAA tier ($133k–$167k) every year.
    fn high_income_medicare_inputs<'a>(
        p: &'a Profile,
        inc: &'a [IncomeSource],
    ) -> ProjectionInputs<'a> {
        let mut inputs = base_inputs(p, &[], inc, &[], &[]);
        inputs.medicare_part_b_annual_premium = 2_220.0;
        inputs
    }

    #[test]
    fn irmaa_has_no_surcharge_for_the_first_two_years_without_lookback_history() {
        let p = profile(1955, 2030 - 1955); // already 71 in 2026
        let inc = [income("pension", 150_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let out = run_projection(&high_income_medicare_inputs(&p, &inc));
        let y2026 = out.annual.iter().find(|r| r.year == 2026).unwrap();
        let y2027 = out.annual.iter().find(|r| r.year == 2027).unwrap();
        assert!(!y2026.irmaa.has_lookback_data);
        assert!(!y2026.irmaa.applies);
        assert_eq!(y2026.irmaa_surcharge, 0.0);
        assert!(!y2027.irmaa.has_lookback_data);
        assert_eq!(y2027.irmaa_surcharge, 0.0);
    }

    #[test]
    fn irmaa_applies_once_two_years_of_in_plan_history_exist() {
        let p = profile(1955, 2030 - 1955);
        let inc = [income("pension", 150_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let out = run_projection(&high_income_medicare_inputs(&p, &inc));
        let y2026 = out.annual.iter().find(|r| r.year == 2026).unwrap();
        let y2028 = out.annual.iter().find(|r| r.year == 2028).unwrap();
        assert!(y2028.irmaa.has_lookback_data);
        assert_eq!(y2028.irmaa.lookback_year, 2026);
        assert!((y2028.irmaa.lookback_magi - y2026.tax.magi).abs() < 1.0);
        assert!(y2028.irmaa.applies);
        // $150k sits in the second single tier ($133k-$167k): $185.00 Part B +
        // $35.30 Part D, one enrolled person, annualized.
        assert_eq!(y2028.irmaa.part_b_surcharge_monthly, 185.00);
        assert_eq!(y2028.irmaa.part_d_surcharge_monthly, 35.30);
        assert_eq!(y2028.irmaa.enrolled_count, 1);
        assert!((y2028.irmaa_surcharge - 2_643.60).abs() < 1.0);
        // The plan runs through 2030 with the same steady-state high MAGI, so
        // 2028, 2029, and 2030 all carry the surcharge once lookback data exists.
        assert!((out.summary.total_lifetime_irmaa_surcharges - 3.0 * y2028.irmaa_surcharge).abs() < 1.0);
    }

    #[test]
    fn irmaa_is_disabled_when_medicare_part_b_is_disabled() {
        let p = profile(1955, 2030 - 1955);
        let inc = [income("pension", 150_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let mut inputs = base_inputs(&p, &[], &inc, &[], &[]);
        inputs.medicare_part_b_annual_premium = 0.0; // Medicare modeling off
        let out = run_projection(&inputs);
        let y2028 = out.annual.iter().find(|r| r.year == 2028).unwrap();
        assert!(!y2028.irmaa.has_lookback_data);
        assert!(!y2028.irmaa.applies);
        assert_eq!(y2028.irmaa_surcharge, 0.0);
    }

    #[test]
    fn irmaa_surcharge_is_charged_per_enrolled_spouse() {
        // Both spouses are 65+ from year one and share the same high MAGI, so
        // once lookback data exists both pay the per-person surcharge.
        let mut p = profile(1955, 2030 - 1955);
        p.marital_status = "married".into();
        p.filing_status = "married_filing_jointly".into();
        p.spouse_date_of_birth = Some(date(1955, 6, 1));
        p.spouse_life_expectancy = Some(75);
        let inc = [income("pension", 250_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let out = run_projection(&high_income_medicare_inputs(&p, &inc));
        let y2028 = out.annual.iter().find(|r| r.year == 2028).unwrap();
        assert!(y2028.irmaa.applies);
        assert_eq!(y2028.irmaa.enrolled_count, 2);
        let per_person_monthly =
            y2028.irmaa.part_b_surcharge_monthly + y2028.irmaa.part_d_surcharge_monthly;
        assert!((y2028.irmaa_surcharge - 2.0 * per_person_monthly * 12.0).abs() < 1.0);
    }

    #[test]
    fn irmaa_uses_the_wider_joint_brackets_for_married_filing_jointly() {
        // $150k combined MAGI sits inside the single filer's second tier but
        // is under the joint schedule's very first threshold ($212k) — a
        // married couple with the same income as the single-filer test above
        // should see no surcharge at all.
        let mut p = profile(1955, 2030 - 1955);
        p.marital_status = "married".into();
        p.filing_status = "married_filing_jointly".into();
        p.spouse_date_of_birth = Some(date(1955, 6, 1));
        p.spouse_life_expectancy = Some(75);
        let inc = [income("pension", 150_000.0, "annual", date(2020, 1, 1), false, 0.0)];
        let out = run_projection(&high_income_medicare_inputs(&p, &inc));
        let y2028 = out.annual.iter().find(|r| r.year == 2028).unwrap();
        assert!(y2028.irmaa.has_lookback_data);
        assert!(!y2028.irmaa.applies);
        assert_eq!(y2028.irmaa_surcharge, 0.0);
    }

    #[test]
    fn zero_shocks_match_the_unshocked_projection() {
        let p = profile(1960, 90);
        let accts = [account("a1", "taxable", 100_000.0, 6.0)];
        let inputs = base_inputs(&p, &accts, &[], &[], &[]);
        let unshocked = run_projection(&inputs);
        let zero_shocks = vec![0.0; unshocked.annual.len()];
        let shocked = run_projection_with_shocks(&inputs, &zero_shocks);

        assert_eq!(unshocked.annual.len(), shocked.annual.len());
        for (a, b) in unshocked.annual.iter().zip(shocked.annual.iter()) {
            assert_eq!(a.year, b.year);
            assert!((a.ending_balance - b.ending_balance).abs() < 1e-9);
        }
    }

    #[test]
    fn a_positive_shock_in_year_zero_increases_that_years_growth() {
        let p = profile(1960, 90);
        let accts = [account("a1", "taxable", 100_000.0, 6.0)];
        let inputs = base_inputs(&p, &accts, &[], &[], &[]);
        let unshocked = run_projection(&inputs);
        let mut shocks = vec![0.0; unshocked.annual.len()];
        shocks[0] = 10.0; // +10 percentage points in the first projected year
        let shocked = run_projection_with_shocks(&inputs, &shocks);

        let unshocked_y0 = &unshocked.annual[0];
        let shocked_y0 = &shocked.annual[0];
        assert_eq!(unshocked_y0.year, shocked_y0.year);
        assert!(shocked_y0.ending_balance > unshocked_y0.ending_balance);
    }

    // ---- ACA-aware Roth conversion --------------------------------------------

    /// A single filer, 60, who gets an ACA subsidy in 2026: 400% of the 2025
    /// poverty line (15,650) is 62,600 of MAGI.
    fn aca_aware_inputs<'a>(
        p: &'a Profile,
        accounts: &'a [Account],
        spend: &'a [SpendingItem],
        mode: &str,
        ceiling: f64,
    ) -> ProjectionInputs<'a> {
        let mut inp = base_inputs(p, accounts, &[], spend, &[]);
        inp.aca_tables = crate::aca::AcaTables::built_in();
        inp.aca_benchmark_annual_premium = 10_000.0;
        inp.roth_conversion_mode = mode.to_string();
        inp.roth_conversion_ceiling = ceiling;
        inp
    }

    fn aca_aware_accounts() -> Vec<Account> {
        vec![
            account("brk", "taxable", 500_000.0, 0.0),
            account("ira", "tax_deferred", 1_000_000.0, 0.0),
            account("roth", "tax_free", 10_000.0, 0.0),
        ]
    }

    #[test]
    fn aca_aware_converts_up_to_the_cutoff_when_spending_does_not_add_to_magi() {
        // Spending comes from a taxable account with no gains, so MAGI is the conversion.
        let p = profile(1966, 2036 - 1966);
        let accts = aca_aware_accounts();
        let spend = [spending("Living", "essential", 30_000.0, "annual", false)];
        let out = run_projection(&aca_aware_inputs(&p, &accts, &spend, "aca_aware", 0.0));
        let y = &out.annual[0];
        assert!((y.roth_conversion - 62_599.0).abs() < 0.01, "{}", y.roth_conversion);
        assert!(y.tax.magi <= 62_600.0);
        assert!(y.aca.eligible);
        assert!(y.aca.subsidy > 0.0);
    }

    #[test]
    fn aca_aware_trims_the_conversion_for_draws_that_fund_spending_and_tax() {
        // Spending and tax come out of the IRA, adding to MAGI on top of the conversion.
        let p = profile(1966, 2036 - 1966);
        let accts = [
            account("ira", "tax_deferred", 1_000_000.0, 0.0),
            account("roth", "tax_free", 10_000.0, 0.0),
        ];
        let spend = [spending("Living", "essential", 30_000.0, "annual", false)];
        let out = run_projection(&aca_aware_inputs(&p, &accts, &spend, "aca_aware", 0.0));
        let y = &out.annual[0];
        println!("conversion {} magi {} subsidy {}", y.roth_conversion, y.tax.magi, y.aca.subsidy);
        assert!(y.roth_conversion > 15_000.0 && y.roth_conversion < 40_000.0);
        assert!(y.tax.magi <= 62_600.0);
        assert!(y.tax.magi > 62_000.0);
        assert!(y.aca.eligible);
        assert!(y.aca.subsidy > 0.0);
    }

    #[test]
    fn aca_aware_flat_ceiling_is_an_extra_cap() {
        let p = profile(1966, 2036 - 1966);
        let accts = aca_aware_accounts();
        let spend = [spending("Living", "essential", 30_000.0, "annual", false)];
        let out = run_projection(&aca_aware_inputs(&p, &accts, &spend, "aca_aware", 20_000.0));
        let y = &out.annual[0];
        assert!(y.tax.taxable_income <= 20_000.0 + 0.01);
        assert!(y.roth_conversion > 0.0);
    }

    #[test]
    fn aca_aware_without_a_cutoff_fills_the_flat_ceiling_like_the_fixed_mode() {
        // 66: no ACA subsidy window, so the cutoff doesn't apply.
        let p = profile(1960, 2036 - 1960);
        let accts = aca_aware_accounts();
        let spend = [spending("Living", "essential", 30_000.0, "annual", false)];
        let aware = run_projection(&aca_aware_inputs(&p, &accts, &spend, "aca_aware", 50_000.0));
        let fixed = run_projection(&aca_aware_inputs(&p, &accts, &spend, "fixed_ceiling", 50_000.0));
        assert!(aware.annual[0].roth_conversion > 0.0);
        assert!((aware.annual[0].roth_conversion - fixed.annual[0].roth_conversion).abs() < 0.01);

        let none = run_projection(&aca_aware_inputs(&p, &accts, &spend, "aca_aware", 0.0));
        assert_eq!(none.annual[0].roth_conversion, 0.0);
    }

    #[test]
    fn fixed_ceiling_ignores_the_aca_cutoff_and_loses_the_subsidy() {
        let p = profile(1966, 2036 - 1966);
        let accts = aca_aware_accounts();
        let spend = [spending("Living", "essential", 30_000.0, "annual", false)];
        let out = run_projection(&aca_aware_inputs(&p, &accts, &spend, "fixed_ceiling", 90_000.0));
        let y = &out.annual[0];
        assert!(y.roth_conversion > 62_600.0);
        assert!(!y.aca.eligible);
    }
}
