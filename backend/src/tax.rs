//! Tax engine (roadmap Phase 2, features 1–5).
//!
//! A pure, unit-tested engine that computes a single year's tax liability:
//!
//! 1. **Federal ordinary income tax** — progressive brackets on ordinary income
//!    (wages, pensions, tax-deferred withdrawals, taxable Social Security, …)
//!    after the standard deduction.
//! 2. **State income tax** — a representative flat rate per state, with Social
//!    Security exempt (as in most states) and no-income-tax states at 0%.
//! 3. **Capital gains** — long-term gains taxed at the preferential 0/15/20%
//!    rates, stacked on top of ordinary taxable income.
//! 4. **Qualified dividends** — taxed at the same preferential rates as
//!    long-term capital gains.
//! 5. **Social Security taxation** — the provisional-income worksheet that makes
//!    up to 85% of benefits taxable.
//!
//! The actual brackets, rates, standard deductions, Social Security thresholds,
//! and state rates are **not hard-coded into the calculation**: they live in a
//! [`TaxTables`] value that is normally loaded from the database (and is
//! admin-maintainable in a later phase). [`TaxTables::default_2025`] holds the
//! built-in 2025 figures used to seed the database and to drive the unit tests,
//! so there is a single source of truth for the numbers.
//!
//! Bracket thresholds, the standard deduction, and the preferential-rate
//! breakpoints are indexed to the plan's general inflation rate from the table's
//! base year, so effective rates stay realistic across a multi-decade
//! projection. The Social Security taxation thresholds are deliberately *not*
//! indexed — by statute they are fixed in nominal dollars, which is why an
//! ever-growing share of benefits becomes taxable over time.
//!
//! Everything here is an approximation suitable for planning, not tax advice:
//! it models the standard deduction only (no itemizing), a flat state rate, and
//! does not yet include NIIT, AMT, or credits (those arrive with later
//! milestones).

use std::collections::HashMap;

/// Federal tax filing status, parsed from the profile's string form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilingStatusKind {
    Single,
    MarriedFilingJointly,
    MarriedFilingSeparately,
    HeadOfHousehold,
    /// Qualifying widow(er) — uses the married-filing-jointly schedule.
    QualifyingWidow,
}

impl FilingStatusKind {
    /// Parse the profile's stored filing-status string, defaulting to single
    /// for anything unrecognized.
    pub fn from_str(s: &str) -> Self {
        match s {
            "married_filing_jointly" => FilingStatusKind::MarriedFilingJointly,
            "married_filing_separately" => FilingStatusKind::MarriedFilingSeparately,
            "head_of_household" => FilingStatusKind::HeadOfHousehold,
            "qualifying_widow" => FilingStatusKind::QualifyingWidow,
            _ => FilingStatusKind::Single,
        }
    }
}

/// One bracket: income at or above `floor` (up to the next bracket's floor) is
/// taxed at `rate` (a fraction, e.g. 0.22).
#[derive(Debug, Clone, Copy)]
pub struct TaxBracket {
    pub floor: f64,
    pub rate: f64,
}

/// Per-filing-status federal scalar parameters.
#[derive(Debug, Clone, Copy)]
pub struct FilingParams {
    pub standard_deduction: f64,
    pub additional_senior_deduction: f64,
    pub ss_base_threshold: f64,
    pub ss_second_threshold: f64,
}

/// Per-state, per-filing-status scalar parameters.
#[derive(Debug, Clone, Copy)]
pub struct StateParams {
    pub standard_deduction: f64,
    pub taxes_social_security: bool,
    pub taxes_capital_gains_as_ordinary: bool,
}

// ---- Plain inputs used to build (and seed) the tables --------------------

/// A federal bracket row as loaded from (or seeded into) the database.
pub struct BracketInput {
    pub bracket_type: String,
    pub filing_status: String,
    pub floor: f64,
    pub rate: f64,
}

/// A per-filing-status parameter row.
pub struct FilingParamInput {
    pub filing_status: String,
    pub standard_deduction: f64,
    pub additional_senior_deduction: f64,
    pub ss_base_threshold: f64,
    pub ss_second_threshold: f64,
}

/// A state bracket row.
pub struct StateBracketInput {
    pub state: String,
    pub filing_status: String,
    pub floor: f64,
    pub rate: f64,
}

/// A per-state, per-filing-status parameter row.
pub struct StateParamInput {
    pub state: String,
    pub filing_status: String,
    pub standard_deduction: f64,
    pub taxes_social_security: bool,
    pub taxes_capital_gains_as_ordinary: bool,
}

/// The full set of tax parameters the engine reads. Built from database rows
/// (see `crate::models::tax`) or from the built-in [`TaxTables::default_2025`].
#[derive(Debug, Clone)]
pub struct TaxTables {
    /// Year the loaded schedules were published for; the engine indexes forward
    /// from here by inflation.
    pub base_year: i32,
    ordinary: HashMap<FilingStatusKind, Vec<TaxBracket>>,
    capital_gains: HashMap<FilingStatusKind, Vec<TaxBracket>>,
    params: HashMap<FilingStatusKind, FilingParams>,
    state_brackets: HashMap<(String, FilingStatusKind), Vec<TaxBracket>>,
    state_params: HashMap<(String, FilingStatusKind), StateParams>,
}

impl TaxTables {
    /// Assemble tables from plain row inputs (as loaded from the database).
    pub fn from_inputs(
        base_year: i32,
        brackets: Vec<BracketInput>,
        params: Vec<FilingParamInput>,
        state_brackets_in: Vec<StateBracketInput>,
        state_params_in: Vec<StateParamInput>,
    ) -> Self {
        let mut ordinary: HashMap<FilingStatusKind, Vec<TaxBracket>> = HashMap::new();
        let mut capital_gains: HashMap<FilingStatusKind, Vec<TaxBracket>> = HashMap::new();
        for b in brackets {
            let fs = FilingStatusKind::from_str(&b.filing_status);
            let target = if b.bracket_type == "capital_gains" {
                &mut capital_gains
            } else {
                &mut ordinary
            };
            target.entry(fs).or_default().push(TaxBracket {
                floor: b.floor,
                rate: b.rate,
            });
        }

        let mut params_map: HashMap<FilingStatusKind, FilingParams> = HashMap::new();
        for p in params {
            params_map.insert(
                FilingStatusKind::from_str(&p.filing_status),
                FilingParams {
                    standard_deduction: p.standard_deduction,
                    additional_senior_deduction: p.additional_senior_deduction,
                    ss_base_threshold: p.ss_base_threshold,
                    ss_second_threshold: p.ss_second_threshold,
                },
            );
        }

        let mut state_brackets: HashMap<(String, FilingStatusKind), Vec<TaxBracket>> =
            HashMap::new();
        for b in state_brackets_in {
            let key = (b.state.to_ascii_uppercase(), FilingStatusKind::from_str(&b.filing_status));
            state_brackets.entry(key).or_default().push(TaxBracket {
                floor: b.floor,
                rate: b.rate,
            });
        }

        let mut state_params: HashMap<(String, FilingStatusKind), StateParams> = HashMap::new();
        for p in state_params_in {
            let key = (p.state.to_ascii_uppercase(), FilingStatusKind::from_str(&p.filing_status));
            state_params.insert(
                key,
                StateParams {
                    standard_deduction: p.standard_deduction,
                    taxes_social_security: p.taxes_social_security,
                    taxes_capital_gains_as_ordinary: p.taxes_capital_gains_as_ordinary,
                },
            );
        }

        // Keep every bracket schedule sorted by ascending floor.
        for v in ordinary.values_mut().chain(capital_gains.values_mut()) {
            v.sort_by(|a, b| a.floor.partial_cmp(&b.floor).unwrap_or(std::cmp::Ordering::Equal));
        }
        for v in state_brackets.values_mut() {
            v.sort_by(|a, b| a.floor.partial_cmp(&b.floor).unwrap_or(std::cmp::Ordering::Equal));
        }

        TaxTables {
            base_year,
            ordinary,
            capital_gains,
            params: params_map,
            state_brackets,
            state_params,
        }
    }

    /// The built-in 2025 tax parameters — the single source of truth used both
    /// to seed the database and to drive unit tests.
    pub fn default_2025() -> Self {
        let (year, brackets, params, state_brackets, state_params) = default_2025_inputs();
        Self::from_inputs(year, brackets, params, state_brackets, state_params)
    }

    /// Ordinary brackets for a filing status (falls back to single).
    fn ordinary_brackets(&self, fs: FilingStatusKind) -> &[TaxBracket] {
        self.ordinary
            .get(&fs)
            .or_else(|| self.ordinary.get(&FilingStatusKind::Single))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Preferential (capital-gain / qualified-dividend) brackets for a status.
    fn capital_gains_brackets(&self, fs: FilingStatusKind) -> &[TaxBracket] {
        self.capital_gains
            .get(&fs)
            .or_else(|| self.capital_gains.get(&FilingStatusKind::Single))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Marginal rate (fraction) the *next* dollar of long-term capital gain or
    /// qualified dividend would face, given the ordinary taxable income it
    /// would stack on top of. Used by the withdrawal engine (roadmap Phase 2,
    /// feature 9) to compare the cost of realizing a gain against drawing an
    /// equivalent ordinary dollar from a tax-deferred account.
    pub fn capital_gains_marginal_rate(
        &self,
        fs: FilingStatusKind,
        year: i32,
        inflation_rate: f64,
        ordinary_taxable_income: f64,
    ) -> f64 {
        let factor = inflation_factor(self.base_year, year, inflation_rate);
        marginal_rate(self.capital_gains_brackets(fs), ordinary_taxable_income, factor)
    }

    /// Scalar parameters for a filing status (falls back to single, then zero).
    fn filing_params(&self, fs: FilingStatusKind) -> FilingParams {
        self.params
            .get(&fs)
            .or_else(|| self.params.get(&FilingStatusKind::Single))
            .copied()
            .unwrap_or(FilingParams {
                standard_deduction: 0.0,
                additional_senior_deduction: 0.0,
                ss_base_threshold: 0.0,
                ss_second_threshold: 0.0,
            })
    }

    /// State brackets for a state + filing status (falls back to the state's
    /// single-filer schedule, then to none).
    fn state_brackets(&self, state: &str, fs: FilingStatusKind) -> &[TaxBracket] {
        let st = state.to_ascii_uppercase();
        self.state_brackets
            .get(&(st.clone(), fs))
            .or_else(|| self.state_brackets.get(&(st, FilingStatusKind::Single)))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// State parameters for a state + filing status (falls back to single, then
    /// to a no-tax default).
    fn state_params(&self, state: &str, fs: FilingStatusKind) -> StateParams {
        let st = state.to_ascii_uppercase();
        self.state_params
            .get(&(st.clone(), fs))
            .or_else(|| self.state_params.get(&(st, FilingStatusKind::Single)))
            .copied()
            .unwrap_or(StateParams {
                standard_deduction: 0.0,
                taxes_social_security: false,
                taxes_capital_gains_as_ordinary: true,
            })
    }

    /// State income tax, computed on the state's own base and brackets. Unlike
    /// the federal calculation, capital gains and qualified dividends are
    /// (by default) taxed as ordinary income, and Social Security is exempt in
    /// most states. Returns an all-zero result for states with no brackets (no
    /// income tax).
    fn state_tax(
        &self,
        state: &str,
        fs: FilingStatusKind,
        ordinary_income_ex_ss: f64,
        preferential: f64,
        taxable_ss: f64,
        factor: f64,
    ) -> StateTaxResult {
        let brackets = self.state_brackets(state, fs);
        if brackets.is_empty() {
            return StateTaxResult::default();
        }
        let params = self.state_params(state, fs);

        let mut income = ordinary_income_ex_ss.max(0.0);
        if params.taxes_capital_gains_as_ordinary {
            income += preferential.max(0.0);
        }
        if params.taxes_social_security {
            income += taxable_ss.max(0.0);
        }

        let deduction = params.standard_deduction * factor;
        let taxable = (income - deduction).max(0.0);
        StateTaxResult {
            tax: bracket_tax_on_slice(brackets, factor, 0.0, taxable),
            taxable_income: taxable,
            standard_deduction: deduction,
            marginal_rate: marginal_rate(brackets, taxable, factor),
        }
    }
}

/// The state portion of a tax computation, mirroring the federal detail.
#[derive(Debug, Clone, Default)]
struct StateTaxResult {
    tax: f64,
    taxable_income: f64,
    standard_deduction: f64,
    marginal_rate: f64,
}

/// Inflation index factor applied to bracket thresholds and the standard
/// deduction for a projection year, relative to the table's base year.
fn inflation_factor(base_year: i32, year: i32, inflation_rate_pct: f64) -> f64 {
    let n = year - base_year;
    if n == 0 {
        1.0
    } else {
        (1.0 + inflation_rate_pct / 100.0).powi(n)
    }
}

/// Portion of Social Security benefits that is taxable, per the IRS provisional
/// income worksheet. `other_agi` is adjusted gross income *excluding* Social
/// Security (ordinary income + qualified dividends + capital gains + any
/// tax-exempt interest).
fn taxable_social_security(base: f64, second: f64, ss_benefits: f64, other_agi: f64) -> f64 {
    if ss_benefits <= 0.0 {
        return 0.0;
    }
    let provisional = other_agi + 0.5 * ss_benefits;

    if provisional <= base {
        0.0
    } else if provisional <= second {
        (0.5 * (provisional - base)).min(0.5 * ss_benefits)
    } else {
        let lower_tier = (0.5 * (second - base)).min(0.5 * ss_benefits);
        (0.85 * (provisional - second) + lower_tier).min(0.85 * ss_benefits)
    }
}

/// Tax on the income slice `[lo, hi)` using a bracket schedule whose floors are
/// scaled by `factor`. Works for both ordinary income (`lo = 0`) and gains
/// stacked on top of ordinary income (`lo = ordinary taxable income`).
fn bracket_tax_on_slice(brackets: &[TaxBracket], factor: f64, lo: f64, hi: f64) -> f64 {
    if hi <= lo {
        return 0.0;
    }
    let mut tax = 0.0;
    for i in 0..brackets.len() {
        let floor = brackets[i].floor * factor;
        let ceil = brackets
            .get(i + 1)
            .map(|b| b.floor * factor)
            .unwrap_or(f64::INFINITY);
        let a = lo.max(floor);
        let b = hi.min(ceil);
        if b > a {
            tax += (b - a) * brackets[i].rate;
        }
    }
    tax
}

/// Marginal rate (fraction) at a given taxable income for a bracket schedule.
fn marginal_rate(brackets: &[TaxBracket], income: f64, factor: f64) -> f64 {
    let mut rate = brackets.first().map(|b| b.rate).unwrap_or(0.0);
    for b in brackets {
        if income > b.floor * factor {
            rate = b.rate;
        } else {
            break;
        }
    }
    rate
}

/// Everything needed to compute one year's tax. Amounts are annual dollars.
pub struct TaxInput<'a> {
    pub year: i32,
    pub filing_status: FilingStatusKind,
    /// General inflation rate (percent) used to index brackets from the base year.
    pub inflation_rate: f64,
    /// Number of taxpayers age 65+ (0, 1, or 2) for the extra standard deduction.
    pub seniors_65_plus: u8,
    /// Ordinary taxable income *excluding* Social Security: taxable income
    /// sources, tax-deferred withdrawals, and taxable one-off inflows.
    pub ordinary_income: f64,
    pub qualified_dividends: f64,
    pub long_term_capital_gains: f64,
    /// Gross Social Security benefits for the year.
    pub social_security_benefits: f64,
    pub state: &'a str,
}

/// The full breakdown of a year's computed tax.
#[derive(Debug, Clone, Default)]
pub struct TaxResult {
    pub taxable_social_security: f64,
    pub adjusted_gross_income: f64,
    pub standard_deduction: f64,
    pub taxable_income: f64,
    pub federal_ordinary_tax: f64,
    pub federal_capital_gains_tax: f64,
    pub federal_tax: f64,
    /// State taxable income (state's own base and standard deduction).
    pub state_taxable_income: f64,
    /// State standard deduction applied (inflation-indexed).
    pub state_standard_deduction: f64,
    pub state_tax: f64,
    /// State marginal rate (fraction) at the top of state taxable income.
    pub state_marginal_rate: f64,
    pub total_tax: f64,
    /// Total tax as a fraction of gross income (0 when there is no income).
    pub effective_rate: f64,
    /// Federal ordinary marginal rate (fraction) at the top of taxable income.
    pub marginal_rate: f64,
}

/// Compute a single year's federal + state tax liability using `tables`.
pub fn compute_taxes(inp: &TaxInput, tables: &TaxTables) -> TaxResult {
    let status = inp.filing_status;
    let factor = inflation_factor(tables.base_year, inp.year, inp.inflation_rate);
    let params = tables.filing_params(status);

    let preferential = inp.qualified_dividends.max(0.0) + inp.long_term_capital_gains.max(0.0);
    let other_agi = inp.ordinary_income.max(0.0) + preferential;

    let taxable_ss = taxable_social_security(
        params.ss_base_threshold,
        params.ss_second_threshold,
        inp.social_security_benefits,
        other_agi,
    );

    // AGI includes the taxable portion of Social Security.
    let agi = other_agi + taxable_ss;

    // Standard deduction, grown for inflation, plus the age-65 add-ons.
    let deduction = (params.standard_deduction
        + params.additional_senior_deduction * inp.seniors_65_plus as f64)
        * factor;

    let taxable_income = (agi - deduction).max(0.0);

    // The standard deduction reduces ordinary income first, then eats into the
    // preferential (gains/dividends) stack.
    let preferential_taxable = preferential.min(taxable_income);
    let ordinary_taxable = taxable_income - preferential_taxable;

    let ordinary_brackets = tables.ordinary_brackets(status);
    let federal_ordinary = bracket_tax_on_slice(ordinary_brackets, factor, 0.0, ordinary_taxable);
    let federal_pref = bracket_tax_on_slice(
        tables.capital_gains_brackets(status),
        factor,
        ordinary_taxable,
        ordinary_taxable + preferential_taxable,
    );
    let federal_tax = federal_ordinary + federal_pref;

    // State tax is computed on the state's own base and brackets — gains and
    // dividends are taxed as ordinary income there, and Social Security is
    // exempt in most states.
    let state = tables.state_tax(
        inp.state,
        status,
        inp.ordinary_income.max(0.0),
        preferential,
        taxable_ss,
        factor,
    );

    let total_tax = federal_tax + state.tax;

    let gross_income = other_agi + inp.social_security_benefits.max(0.0);
    let effective_rate = if gross_income > 0.0 {
        total_tax / gross_income
    } else {
        0.0
    };

    TaxResult {
        taxable_social_security: taxable_ss,
        adjusted_gross_income: agi,
        standard_deduction: deduction,
        taxable_income,
        federal_ordinary_tax: federal_ordinary,
        federal_capital_gains_tax: federal_pref,
        federal_tax,
        state_taxable_income: state.taxable_income,
        state_standard_deduction: state.standard_deduction,
        state_tax: state.tax,
        state_marginal_rate: state.marginal_rate,
        total_tax,
        effective_rate,
        marginal_rate: marginal_rate(ordinary_brackets, ordinary_taxable, factor),
    }
}

/// The built-in 2025 tax parameters as plain rows: `(base_year, federal
/// brackets, per-status params, state rates)`. This is the single source of
/// truth used both to seed the `tax_*` tables and to build
/// [`TaxTables::default_2025`].
pub fn default_2025_inputs() -> (
    i32,
    Vec<BracketInput>,
    Vec<FilingParamInput>,
    Vec<StateBracketInput>,
    Vec<StateParamInput>,
) {
    let ord = |filing_status: &str, rows: &[(f64, f64)]| -> Vec<BracketInput> {
        rows.iter()
            .map(|&(floor, rate)| BracketInput {
                bracket_type: "ordinary".into(),
                filing_status: filing_status.into(),
                floor,
                rate,
            })
            .collect()
    };
    let cg = |filing_status: &str, rows: &[(f64, f64)]| -> Vec<BracketInput> {
        rows.iter()
            .map(|&(floor, rate)| BracketInput {
                bracket_type: "capital_gains".into(),
                filing_status: filing_status.into(),
                floor,
                rate,
            })
            .collect()
    };

    let mut brackets: Vec<BracketInput> = Vec::new();

    // Federal 2025 ordinary-income brackets.
    brackets.extend(ord(
        "single",
        &[
            (0.0, 0.10),
            (11_925.0, 0.12),
            (48_475.0, 0.22),
            (103_350.0, 0.24),
            (197_300.0, 0.32),
            (250_525.0, 0.35),
            (626_350.0, 0.37),
        ],
    ));
    brackets.extend(ord(
        "married_filing_jointly",
        &[
            (0.0, 0.10),
            (23_850.0, 0.12),
            (96_950.0, 0.22),
            (206_700.0, 0.24),
            (394_600.0, 0.32),
            (501_050.0, 0.35),
            (751_600.0, 0.37),
        ],
    ));
    // Qualifying surviving spouse uses the married-filing-jointly schedule.
    brackets.extend(ord(
        "qualifying_widow",
        &[
            (0.0, 0.10),
            (23_850.0, 0.12),
            (96_950.0, 0.22),
            (206_700.0, 0.24),
            (394_600.0, 0.32),
            (501_050.0, 0.35),
            (751_600.0, 0.37),
        ],
    ));
    brackets.extend(ord(
        "married_filing_separately",
        &[
            (0.0, 0.10),
            (11_925.0, 0.12),
            (48_475.0, 0.22),
            (103_350.0, 0.24),
            (197_300.0, 0.32),
            (250_525.0, 0.35),
            (375_800.0, 0.37),
        ],
    ));
    brackets.extend(ord(
        "head_of_household",
        &[
            (0.0, 0.10),
            (17_000.0, 0.12),
            (64_850.0, 0.22),
            (103_350.0, 0.24),
            (197_300.0, 0.32),
            (250_500.0, 0.35),
            (626_350.0, 0.37),
        ],
    ));

    // Federal 2025 preferential (long-term capital gain / qualified dividend)
    // brackets: 0% up to the first breakpoint, 15% up to the second, then 20%.
    brackets.extend(cg(
        "single",
        &[(0.0, 0.0), (48_350.0, 0.15), (533_400.0, 0.20)],
    ));
    brackets.extend(cg(
        "married_filing_jointly",
        &[(0.0, 0.0), (96_700.0, 0.15), (600_050.0, 0.20)],
    ));
    brackets.extend(cg(
        "qualifying_widow",
        &[(0.0, 0.0), (96_700.0, 0.15), (600_050.0, 0.20)],
    ));
    brackets.extend(cg(
        "married_filing_separately",
        &[(0.0, 0.0), (48_350.0, 0.15), (300_000.0, 0.20)],
    ));
    brackets.extend(cg(
        "head_of_household",
        &[(0.0, 0.0), (64_750.0, 0.15), (566_700.0, 0.20)],
    ));

    // Per-filing-status parameters: standard deduction, age-65 add-on, and the
    // Social Security provisional-income thresholds.
    let params = vec![
        FilingParamInput {
            filing_status: "single".into(),
            standard_deduction: 15_000.0,
            additional_senior_deduction: 2_000.0,
            ss_base_threshold: 25_000.0,
            ss_second_threshold: 34_000.0,
        },
        FilingParamInput {
            filing_status: "married_filing_jointly".into(),
            standard_deduction: 30_000.0,
            additional_senior_deduction: 1_600.0,
            ss_base_threshold: 32_000.0,
            ss_second_threshold: 44_000.0,
        },
        FilingParamInput {
            filing_status: "qualifying_widow".into(),
            standard_deduction: 30_000.0,
            additional_senior_deduction: 1_600.0,
            ss_base_threshold: 32_000.0,
            ss_second_threshold: 44_000.0,
        },
        FilingParamInput {
            filing_status: "married_filing_separately".into(),
            standard_deduction: 15_000.0,
            additional_senior_deduction: 1_600.0,
            // Living with a spouse, MFS gets no exclusion; benefits are taxed
            // from the first dollar.
            ss_base_threshold: 0.0,
            ss_second_threshold: 0.0,
        },
        FilingParamInput {
            filing_status: "head_of_household".into(),
            standard_deduction: 22_500.0,
            additional_senior_deduction: 2_000.0,
            ss_base_threshold: 25_000.0,
            ss_second_threshold: 34_000.0,
        },
    ];

    // ---- State income tax ------------------------------------------------
    //
    // Each state is modeled on its own base: its own brackets and standard
    // deduction, Social Security exempt in most states, and (by default)
    // long-term gains and qualified dividends taxed as ordinary income. Where a
    // filing status is not listed for a state, lookups fall back to that
    // state's single-filer schedule.
    let mut state_brackets: Vec<StateBracketInput> = Vec::new();
    let mut state_params: Vec<StateParamInput> = Vec::new();

    // Push one progressive schedule (and its standard deduction) for a state +
    // filing status. Social Security is exempt and gains are ordinary.
    let mut add_state = |state: &str,
                         filing_status: &str,
                         standard_deduction: f64,
                         rows: &[(f64, f64)]| {
        for &(floor, rate) in rows {
            state_brackets.push(StateBracketInput {
                state: state.into(),
                filing_status: filing_status.into(),
                floor,
                rate,
            });
        }
        state_params.push(StateParamInput {
            state: state.into(),
            filing_status: filing_status.into(),
            standard_deduction,
            taxes_social_security: false,
            taxes_capital_gains_as_ordinary: true,
        });
    };

    // California — full 2024 schedule (indexed forward by inflation). The 1%
    // mental-health surcharge on taxable income over $1M is not modeled.
    // Married-filing-separately falls back to the single schedule; qualifying
    // widow(er) uses the joint schedule.
    add_state(
        "CA",
        "single",
        5_540.0,
        &[
            (0.0, 0.01),
            (10_756.0, 0.02),
            (25_499.0, 0.04),
            (40_245.0, 0.06),
            (55_866.0, 0.08),
            (70_606.0, 0.093),
            (360_659.0, 0.103),
            (432_787.0, 0.113),
            (721_314.0, 0.123),
        ],
    );
    let ca_joint: &[(f64, f64)] = &[
        (0.0, 0.01),
        (21_512.0, 0.02),
        (50_998.0, 0.04),
        (80_490.0, 0.06),
        (111_732.0, 0.08),
        (141_212.0, 0.093),
        (721_318.0, 0.103),
        (865_574.0, 0.113),
        (1_442_628.0, 0.123),
    ];
    add_state("CA", "married_filing_jointly", 11_080.0, ca_joint);
    add_state("CA", "qualifying_widow", 11_080.0, ca_joint);
    add_state(
        "CA",
        "head_of_household",
        11_080.0,
        &[
            (0.0, 0.01),
            (21_527.0, 0.02),
            (51_000.0, 0.04),
            (65_744.0, 0.06),
            (81_364.0, 0.08),
            (96_107.0, 0.093),
            (490_493.0, 0.103),
            (588_593.0, 0.113),
            (980_987.0, 0.123),
        ],
    );

    // Remaining states as single-bracket flat-rate approximations (0 for the
    // nine states with no income tax). Only a single-filer row is emitted; the
    // other statuses fall back to it. These flat rates are a deliberate
    // simplification, to be replaced by full per-state schedules over time.
    let flat_states: &[(&str, f64)] = &[
        ("AL", 0.05),
        ("AK", 0.0),
        ("AZ", 0.025),
        ("AR", 0.039),
        ("CO", 0.044),
        ("CT", 0.05),
        ("DE", 0.0555),
        ("DC", 0.065),
        ("FL", 0.0),
        ("GA", 0.0539),
        ("HI", 0.076),
        ("ID", 0.058),
        ("IL", 0.0495),
        ("IN", 0.03),
        ("IA", 0.038),
        ("KS", 0.0525),
        ("KY", 0.04),
        ("LA", 0.03),
        ("ME", 0.0675),
        ("MD", 0.0475),
        ("MA", 0.05),
        ("MI", 0.0425),
        ("MN", 0.068),
        ("MS", 0.044),
        ("MO", 0.047),
        ("MT", 0.059),
        ("NE", 0.0501),
        ("NV", 0.0),
        ("NH", 0.0),
        ("NJ", 0.05525),
        ("NM", 0.047),
        ("NY", 0.0585),
        ("NC", 0.0425),
        ("ND", 0.0195),
        ("OH", 0.035),
        ("OK", 0.0475),
        ("OR", 0.0875),
        ("PA", 0.0307),
        ("RI", 0.0475),
        ("SC", 0.062),
        ("SD", 0.0),
        ("TN", 0.0),
        ("TX", 0.0),
        ("UT", 0.0455),
        ("VT", 0.066),
        ("VA", 0.0575),
        ("WA", 0.0),
        ("WV", 0.045),
        ("WI", 0.053),
        ("WY", 0.0),
    ];
    for &(state, rate) in flat_states {
        add_state(state, "single", 0.0, &[(0.0, rate)]);
    }

    (2025, brackets, params, state_brackets, state_params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> TaxTables {
        TaxTables::default_2025()
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1.0, "expected ~{b}, got {a}");
    }

    fn input(status: FilingStatusKind, ordinary: f64) -> TaxInput<'static> {
        TaxInput {
            year: 2025,
            filing_status: status,
            inflation_rate: 0.0,
            seniors_65_plus: 0,
            ordinary_income: ordinary,
            qualified_dividends: 0.0,
            long_term_capital_gains: 0.0,
            social_security_benefits: 0.0,
            state: "TX",
        }
    }

    #[test]
    fn standard_deduction_shields_low_income() {
        let out = compute_taxes(&input(FilingStatusKind::Single, 12_000.0), &tables());
        assert_eq!(out.taxable_income, 0.0);
        assert_eq!(out.federal_tax, 0.0);
        assert_eq!(out.total_tax, 0.0);
    }

    #[test]
    fn single_ordinary_brackets_are_progressive() {
        // $65k ordinary, single. Taxable = 50,000.
        // 10%*11,925 + 12%*36,550 + 22%*1,525 = 5,914.
        let out = compute_taxes(&input(FilingStatusKind::Single, 65_000.0), &tables());
        approx(out.taxable_income, 50_000.0);
        approx(out.federal_ordinary_tax, 5_914.0);
        assert!((out.marginal_rate - 0.22).abs() < 1e-9);
    }

    #[test]
    fn married_filing_jointly_uses_wider_brackets() {
        // $65k ordinary, MFJ. Taxable = 35,000.
        // 10%*23,850 + 12%*11,150 = 3,723.
        let out = compute_taxes(&input(FilingStatusKind::MarriedFilingJointly, 65_000.0), &tables());
        approx(out.taxable_income, 35_000.0);
        approx(out.federal_ordinary_tax, 3_723.0);
    }

    #[test]
    fn married_filing_separately_has_its_own_top_bracket() {
        // At $400k taxable, MFS reaches the 37% bracket (floor 375,800) while a
        // single filer is still in 35% (37% floor 626,350).
        let out_mfs = compute_taxes(
            &input(FilingStatusKind::MarriedFilingSeparately, 415_000.0),
            &tables(),
        );
        assert!((out_mfs.marginal_rate - 0.37).abs() < 1e-9);
        let out_single =
            compute_taxes(&input(FilingStatusKind::Single, 415_000.0), &tables());
        assert!((out_single.marginal_rate - 0.35).abs() < 1e-9);
    }

    #[test]
    fn senior_deduction_reduces_taxable_income() {
        let mut i = input(FilingStatusKind::Single, 20_000.0);
        i.seniors_65_plus = 1;
        let out = compute_taxes(&i, &tables());
        approx(out.taxable_income, 3_000.0); // 15,000 + 2,000 deduction
    }

    #[test]
    fn qualified_dividends_get_zero_rate_when_income_is_low() {
        let mut i = input(FilingStatusKind::Single, 15_000.0);
        i.qualified_dividends = 20_000.0;
        let out = compute_taxes(&i, &tables());
        approx(out.taxable_income, 20_000.0);
        approx(out.federal_ordinary_tax, 0.0);
        approx(out.federal_capital_gains_tax, 0.0);
    }

    #[test]
    fn long_term_gains_taxed_at_15_percent_when_stacked_high() {
        let mut i = input(FilingStatusKind::Single, 80_000.0);
        i.long_term_capital_gains = 20_000.0;
        let out = compute_taxes(&i, &tables());
        approx(out.federal_capital_gains_tax, 3_000.0);
    }

    #[test]
    fn gains_split_across_zero_and_fifteen_percent_bands() {
        let mut i = input(FilingStatusKind::Single, 15_000.0);
        i.long_term_capital_gains = 40_000.0;
        let out = compute_taxes(&i, &tables());
        approx(out.federal_capital_gains_tax, 0.0);

        // Ordinary taxable 30k + 40k gains: 0% up to 48,350, remainder at 15%.
        let mut i2 = input(FilingStatusKind::Single, 45_000.0);
        i2.long_term_capital_gains = 40_000.0;
        let out2 = compute_taxes(&i2, &tables());
        approx(out2.federal_capital_gains_tax, 3_247.5);
    }

    #[test]
    fn social_security_not_taxable_below_base() {
        let mut i = input(FilingStatusKind::Single, 0.0);
        i.social_security_benefits = 20_000.0;
        let out = compute_taxes(&i, &tables());
        assert_eq!(out.taxable_social_security, 0.0);
        assert_eq!(out.total_tax, 0.0);
    }

    #[test]
    fn social_security_partially_taxable_in_middle_tier() {
        let mut i = input(FilingStatusKind::Single, 30_000.0);
        i.social_security_benefits = 20_000.0;
        let out = compute_taxes(&i, &tables());
        approx(out.taxable_social_security, 9_600.0);
    }

    #[test]
    fn social_security_caps_at_85_percent() {
        let mut i = input(FilingStatusKind::Single, 200_000.0);
        i.social_security_benefits = 40_000.0;
        let out = compute_taxes(&i, &tables());
        approx(out.taxable_social_security, 0.85 * 40_000.0);
    }

    #[test]
    fn state_tax_zero_in_no_tax_states() {
        let out = compute_taxes(&input(FilingStatusKind::Single, 100_000.0), &tables());
        assert_eq!(out.state_tax, 0.0); // TX
    }

    #[test]
    fn california_uses_its_own_progressive_brackets() {
        // Single, $65k ordinary. CA taxable = 65,000 − 5,540 = 59,460 through
        // CA's 1%–8% bands: 107.56 + 294.86 + 589.84 + 937.26 + 287.52 = 2,217.04.
        let mut i = input(FilingStatusKind::Single, 65_000.0);
        i.state = "CA";
        let out = compute_taxes(&i, &tables());
        approx(out.state_tax, 2_217.04);
    }

    #[test]
    fn california_taxes_capital_gains_as_ordinary() {
        // Single, $50k long-term gains, no other income. Federally the gains sit
        // in the 0% preferential bracket (no federal tax), but CA taxes them as
        // ordinary income, so state tax is positive.
        let mut i = input(FilingStatusKind::Single, 0.0);
        i.long_term_capital_gains = 50_000.0;
        i.state = "CA";
        let out = compute_taxes(&i, &tables());
        assert_eq!(out.federal_capital_gains_tax, 0.0);
        // CA taxable = 50,000 − 5,540 = 44,460: 107.56 + 294.86 + 589.84 + 252.90.
        approx(out.state_tax, 1_245.16);
    }

    #[test]
    fn california_exempts_social_security() {
        // Adding Social Security benefits does not change CA state tax, even
        // though it raises federal tax.
        let mut base = input(FilingStatusKind::Single, 40_000.0);
        base.state = "CA";
        let without_ss = compute_taxes(&base, &tables());

        let mut with_ss = input(FilingStatusKind::Single, 40_000.0);
        with_ss.state = "CA";
        with_ss.social_security_benefits = 30_000.0;
        let out = compute_taxes(&with_ss, &tables());

        approx(out.state_tax, without_ss.state_tax);
        assert!(out.federal_tax > without_ss.federal_tax);
    }

    #[test]
    fn flat_state_taxes_all_income_at_its_rate() {
        // Colorado: flat 4.4% with no state standard deduction in this model.
        let mut i = input(FilingStatusKind::Single, 65_000.0);
        i.state = "CO";
        let out = compute_taxes(&i, &tables());
        approx(out.state_tax, 65_000.0 * 0.044);
    }

    #[test]
    fn brackets_are_inflation_indexed() {
        let mut i = input(FilingStatusKind::Single, 65_000.0);
        i.year = 2045;
        i.inflation_rate = 3.0;
        let out = compute_taxes(&i, &tables());
        assert!(out.taxable_income < 40_000.0);
        assert!(out.standard_deduction > 26_000.0);
    }

    #[test]
    fn effective_rate_is_total_over_gross() {
        let out = compute_taxes(&input(FilingStatusKind::Single, 65_000.0), &tables());
        approx(out.effective_rate * 65_000.0, out.total_tax);
    }

    #[test]
    fn state_lookup_is_case_insensitive_and_falls_back_to_single() {
        let t = TaxTables::default_2025();
        assert_eq!(t.base_year, 2025);
        // Lower-case state code resolves; married-filing-separately falls back to
        // California's single schedule (same result as single).
        let mut lc = input(FilingStatusKind::Single, 65_000.0);
        lc.state = "ca";
        let mut mfs = input(FilingStatusKind::MarriedFilingSeparately, 65_000.0);
        mfs.state = "CA";
        approx(
            compute_taxes(&lc, &t).state_tax,
            compute_taxes(&mfs, &t).state_tax,
        );
    }

    #[test]
    fn capital_gains_marginal_rate_matches_the_bracket_at_the_stacking_point() {
        let t = tables();
        // Single, 2025: 0% up to 48,350, 15% up to 533,400, then 20%.
        assert_eq!(
            t.capital_gains_marginal_rate(FilingStatusKind::Single, 2025, 0.0, 0.0),
            0.0
        );
        assert_eq!(
            t.capital_gains_marginal_rate(FilingStatusKind::Single, 2025, 0.0, 100_000.0),
            0.15
        );
        assert_eq!(
            t.capital_gains_marginal_rate(FilingStatusKind::Single, 2025, 0.0, 600_000.0),
            0.20
        );
    }
}
