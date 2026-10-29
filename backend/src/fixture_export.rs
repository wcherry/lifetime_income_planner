//! Golden parity fixtures for the native macOS rewrite
//! (`../lifetime_income_planner_macos`). Runs the real engines over a fixed
//! set of scenarios and input grids and writes `{inputs, expected}` JSON that
//! the Swift port's tests replay and compare field by field.
//!
//! Test-only and ignored by default; run it through
//! `cargo xtask export-fixtures --out <dir>`, which sets `FIXTURE_OUT_DIR`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde_json::{json, Value};

use crate::aca::{compute_subsidy, AcaInput, AcaTables};
use crate::insights::{generate_insights, InsightContext, ReviewVariance};
use crate::irmaa::{compute_irmaa, IrmaaFilingGroup, IrmaaInput, IrmaaTables};
use crate::models::{
    Account, IncomeSource, LifeEvent, Profile, SpendingItem, DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM,
    DEFAULT_AVOID_EARLY_WITHDRAWAL_PENALTY, DEFAULT_HEALTHCARE_INFLATION_RATE,
    DEFAULT_INFLATION_RATE, DEFAULT_INVESTMENT_RETURN_RATE, DEFAULT_MEDICARE_ENROLLMENT_AGE,
    DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM, DEFAULT_ROTH_CONVERSION_CEILING,
    DEFAULT_ROTH_CONVERSION_MODE, DEFAULT_SOCIAL_SECURITY_COLA_RATE, DEFAULT_WITHDRAWAL_STRATEGY,
};
use crate::monte_carlo::run_monte_carlo;
use crate::projection::{run_projection, run_projection_with_shocks, ProjectionInputs};
use crate::tasks::{derive_tasks, TaskContext};
use crate::tax::{compute_taxes, FilingStatusKind, TaxInput, TaxTables};

const CURRENT_YEAR: i32 = 2026;

// ---- Builders --------------------------------------------------------------

fn ts() -> NaiveDateTime {
    date(2026, 1, 1).and_hms_opt(0, 0, 0).unwrap()
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn profile(dob: NaiveDate, life_expectancy: i32, filing: &str, state: &str) -> Profile {
    Profile {
        id: "p1".into(),
        user_id: "u1".into(),
        first_name: "Pat".into(),
        last_name: "Planner".into(),
        date_of_birth: dob,
        marital_status: if filing == "married_filing_jointly" || filing == "married_filing_separately" {
            "married".into()
        } else if filing == "qualifying_widow" {
            "widowed".into()
        } else {
            "single".into()
        },
        filing_status: filing.into(),
        state: state.into(),
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

fn with_spouse(mut p: Profile, dob: NaiveDate, life_expectancy: i32) -> Profile {
    p.spouse_first_name = Some("Sam".into());
    p.spouse_last_name = Some("Planner".into());
    p.spouse_date_of_birth = Some(dob);
    p.spouse_life_expectancy = Some(life_expectancy);
    p
}

#[allow(clippy::too_many_arguments)]
fn account(
    id: &str,
    category: &str,
    account_type: &str,
    owner: &str,
    balance: f64,
    roi: f64,
    dividend_yield: f64,
    cost_basis: Option<f64>,
) -> Account {
    Account {
        id: id.into(),
        user_id: "u1".into(),
        name: format!("{id} account"),
        category: category.into(),
        account_type: account_type.into(),
        owner: owner.into(),
        current_balance: balance,
        expected_roi: roi,
        dividend_yield,
        cost_basis,
        allocation_stock_pct: Some(60),
        allocation_bond_pct: Some(35),
        allocation_cash_pct: Some(5),
        withdrawal_restrictions: None,
        created_at: ts(),
        updated_at: ts(),
    }
}

#[allow(clippy::too_many_arguments)]
fn income(
    id: &str,
    income_type: &str,
    owner: &str,
    amount: f64,
    frequency: &str,
    start: NaiveDate,
    end: Option<NaiveDate>,
    growth: f64,
    cola: bool,
    taxability: &str,
) -> IncomeSource {
    IncomeSource {
        id: id.into(),
        user_id: "u1".into(),
        name: format!("{id} income"),
        income_type: income_type.into(),
        owner: owner.into(),
        amount,
        frequency: frequency.into(),
        start_date: start,
        end_date: end,
        growth_rate: growth,
        cola,
        taxability: taxability.into(),
        notes: None,
        created_at: ts(),
        updated_at: ts(),
    }
}

fn social_security(id: &str, owner: &str, monthly: f64, start: NaiveDate) -> IncomeSource {
    income(id, "social_security", owner, monthly, "monthly", start, None, 0.0, true, "taxable")
}

#[allow(clippy::too_many_arguments)]
fn spending(
    id: &str,
    category: &str,
    amount: f64,
    frequency: &str,
    inflation_adjusted: bool,
    start_year: Option<i32>,
    end_year: Option<i32>,
) -> SpendingItem {
    SpendingItem {
        id: id.into(),
        user_id: "u1".into(),
        name: format!("{id} spending"),
        category: category.into(),
        amount,
        frequency: frequency.into(),
        inflation_adjusted,
        start_year,
        end_year,
        notes: None,
        created_at: ts(),
        updated_at: ts(),
    }
}

fn living(annual: f64) -> SpendingItem {
    spending("living", "essential", annual, "annual", true, None, None)
}

#[allow(clippy::too_many_arguments)]
fn event(
    id: &str,
    event_type: &str,
    event_date: NaiveDate,
    direction: &str,
    amount: f64,
    taxable: bool,
    inflation_adjusted: bool,
    recurrence: &str,
    end_date: Option<NaiveDate>,
) -> LifeEvent {
    LifeEvent {
        id: id.into(),
        user_id: "u1".into(),
        name: format!("{id} event"),
        event_type: event_type.into(),
        event_date,
        direction: direction.into(),
        amount,
        taxable,
        inflation_adjusted,
        recurrence: recurrence.into(),
        end_date,
        notes: None,
        created_at: ts(),
        updated_at: ts(),
    }
}

#[derive(Clone)]
struct Assume {
    inflation_rate: f64,
    investment_return_rate: f64,
    healthcare_inflation_rate: f64,
    social_security_cola_rate: f64,
    roth_conversion_ceiling: f64,
    roth_conversion_start_year: Option<i32>,
    roth_conversion_end_year: Option<i32>,
    withdrawal_strategy: &'static str,
    aca_benchmark_annual_premium: f64,
    medicare_part_b_annual_premium: f64,
    avoid_early_withdrawal_penalty: bool,
    medicare_enrollment_age: i32,
    roth_conversion_mode: &'static str,
}

impl Default for Assume {
    fn default() -> Self {
        Assume {
            inflation_rate: DEFAULT_INFLATION_RATE,
            investment_return_rate: DEFAULT_INVESTMENT_RETURN_RATE,
            healthcare_inflation_rate: DEFAULT_HEALTHCARE_INFLATION_RATE,
            social_security_cola_rate: DEFAULT_SOCIAL_SECURITY_COLA_RATE,
            roth_conversion_ceiling: DEFAULT_ROTH_CONVERSION_CEILING,
            roth_conversion_start_year: None,
            roth_conversion_end_year: None,
            withdrawal_strategy: DEFAULT_WITHDRAWAL_STRATEGY,
            aca_benchmark_annual_premium: DEFAULT_ACA_BENCHMARK_ANNUAL_PREMIUM,
            medicare_part_b_annual_premium: DEFAULT_MEDICARE_PART_B_ANNUAL_PREMIUM,
            avoid_early_withdrawal_penalty: DEFAULT_AVOID_EARLY_WITHDRAWAL_PENALTY,
            medicare_enrollment_age: DEFAULT_MEDICARE_ENROLLMENT_AGE,
            roth_conversion_mode: DEFAULT_ROTH_CONVERSION_MODE,
        }
    }
}

struct Scenario {
    name: &'static str,
    description: &'static str,
    profile: Profile,
    accounts: Vec<Account>,
    income: Vec<IncomeSource>,
    spending: Vec<SpendingItem>,
    life_events: Vec<LifeEvent>,
    assumptions: Assume,
    assumptions_are_default: bool,
    shocks: Option<Vec<f64>>,
}

impl Scenario {
    fn new(name: &'static str, description: &'static str, profile: Profile) -> Self {
        Scenario {
            name,
            description,
            profile,
            accounts: vec![],
            income: vec![],
            spending: vec![],
            life_events: vec![],
            assumptions: Assume::default(),
            assumptions_are_default: false,
            shocks: None,
        }
    }

    fn inputs(&self) -> ProjectionInputs<'_> {
        let a = &self.assumptions;
        ProjectionInputs {
            current_year: CURRENT_YEAR,
            profile: &self.profile,
            accounts: &self.accounts,
            income: &self.income,
            spending: &self.spending,
            life_events: &self.life_events,
            inflation_rate: a.inflation_rate,
            investment_return_rate: a.investment_return_rate,
            healthcare_inflation_rate: a.healthcare_inflation_rate,
            social_security_cola_rate: a.social_security_cola_rate,
            assumptions_are_default: self.assumptions_are_default,
            roth_conversion_ceiling: a.roth_conversion_ceiling,
            roth_conversion_start_year: a.roth_conversion_start_year,
            roth_conversion_end_year: a.roth_conversion_end_year,
            withdrawal_strategy: a.withdrawal_strategy.to_string(),
            aca_benchmark_annual_premium: a.aca_benchmark_annual_premium,
            medicare_part_b_annual_premium: a.medicare_part_b_annual_premium,
            avoid_early_withdrawal_penalty: a.avoid_early_withdrawal_penalty,
            medicare_enrollment_age: a.medicare_enrollment_age,
            roth_conversion_mode: a.roth_conversion_mode.to_string(),
            tax_tables: TaxTables::default_2025(),
            aca_tables: AcaTables::default_2025(),
            irmaa_tables: IrmaaTables::default_2025(),
        }
    }
}

// ---- JSON encoding of the (non-Serialize) input models ---------------------

fn d(v: &NaiveDate) -> Value {
    Value::String(v.to_string())
}

fn profile_json(p: &Profile) -> Value {
    json!({
        "first_name": p.first_name,
        "last_name": p.last_name,
        "date_of_birth": d(&p.date_of_birth),
        "marital_status": p.marital_status,
        "filing_status": p.filing_status,
        "state": p.state,
        "retirement_date": d(&p.retirement_date),
        "life_expectancy": p.life_expectancy,
        "spouse_first_name": p.spouse_first_name,
        "spouse_last_name": p.spouse_last_name,
        "spouse_date_of_birth": p.spouse_date_of_birth.as_ref().map(d),
        "spouse_life_expectancy": p.spouse_life_expectancy,
    })
}

fn account_json(a: &Account) -> Value {
    json!({
        "id": a.id,
        "name": a.name,
        "category": a.category,
        "account_type": a.account_type,
        "owner": a.owner,
        "current_balance": a.current_balance,
        "expected_roi": a.expected_roi,
        "dividend_yield": a.dividend_yield,
        "cost_basis": a.cost_basis,
        "allocation_stock_pct": a.allocation_stock_pct,
        "allocation_bond_pct": a.allocation_bond_pct,
        "allocation_cash_pct": a.allocation_cash_pct,
        "withdrawal_restrictions": a.withdrawal_restrictions,
    })
}

fn income_json(s: &IncomeSource) -> Value {
    json!({
        "id": s.id,
        "name": s.name,
        "income_type": s.income_type,
        "owner": s.owner,
        "amount": s.amount,
        "frequency": s.frequency,
        "start_date": d(&s.start_date),
        "end_date": s.end_date.as_ref().map(d),
        "growth_rate": s.growth_rate,
        "cola": s.cola,
        "taxability": s.taxability,
        "notes": s.notes,
    })
}

fn spending_json(s: &SpendingItem) -> Value {
    json!({
        "id": s.id,
        "name": s.name,
        "category": s.category,
        "amount": s.amount,
        "frequency": s.frequency,
        "inflation_adjusted": s.inflation_adjusted,
        "start_year": s.start_year,
        "end_year": s.end_year,
        "notes": s.notes,
    })
}

fn life_event_json(e: &LifeEvent) -> Value {
    json!({
        "id": e.id,
        "name": e.name,
        "event_type": e.event_type,
        "event_date": d(&e.event_date),
        "direction": e.direction,
        "amount": e.amount,
        "taxable": e.taxable,
        "inflation_adjusted": e.inflation_adjusted,
        "recurrence": e.recurrence,
        "end_date": e.end_date.as_ref().map(d),
        "notes": e.notes,
    })
}

fn assumptions_json(a: &Assume) -> Value {
    json!({
        "inflation_rate": a.inflation_rate,
        "investment_return_rate": a.investment_return_rate,
        "healthcare_inflation_rate": a.healthcare_inflation_rate,
        "social_security_cola_rate": a.social_security_cola_rate,
        "roth_conversion_ceiling": a.roth_conversion_ceiling,
        "roth_conversion_start_year": a.roth_conversion_start_year,
        "roth_conversion_end_year": a.roth_conversion_end_year,
        "withdrawal_strategy": a.withdrawal_strategy,
        "aca_benchmark_annual_premium": a.aca_benchmark_annual_premium,
        "medicare_part_b_annual_premium": a.medicare_part_b_annual_premium,
        "avoid_early_withdrawal_penalty": a.avoid_early_withdrawal_penalty,
        "medicare_enrollment_age": a.medicare_enrollment_age,
        "roth_conversion_mode": a.roth_conversion_mode,
    })
}

fn filing_str(fs: FilingStatusKind) -> &'static str {
    match fs {
        FilingStatusKind::Single => "single",
        FilingStatusKind::MarriedFilingJointly => "married_filing_jointly",
        FilingStatusKind::MarriedFilingSeparately => "married_filing_separately",
        FilingStatusKind::HeadOfHousehold => "head_of_household",
        FilingStatusKind::QualifyingWidow => "qualifying_widow",
    }
}

fn group_str(g: IrmaaFilingGroup) -> &'static str {
    match g {
        IrmaaFilingGroup::Single => "single",
        IrmaaFilingGroup::MarriedFilingJointly => "married_filing_jointly",
        IrmaaFilingGroup::MarriedFilingSeparately => "married_filing_separately",
    }
}

// ---- Scenarios ---------------------------------------------------------------

fn scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();

    // A typical single retiree: all three tax buckets, Social Security, a
    // pension, mixed spending.
    let mut s = Scenario::new(
        "single_tx_baseline",
        "Single filer in Texas with taxable, IRA, and Roth accounts, Social Security and a pension.",
        profile(date(1961, 3, 15), 92, "single", "TX"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 350_000.0, 6.0, 1.8, Some(210_000.0)),
        account("ira", "tax_deferred", "ira", "self", 650_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 120_000.0, 6.5, 0.0, None),
        account("checking", "taxable", "checking", "self", 25_000.0, 0.5, 0.0, None),
    ];
    s.income = vec![
        social_security("ss", "self", 2_650.0, date(2028, 3, 1)),
        income("pension", "pension", "self", 1_400.0, "monthly", date(2026, 1, 1), None, 0.0, false, "taxable"),
    ];
    s.spending = vec![
        living(48_000.0),
        spending("health", "healthcare", 650.0, "monthly", true, None, None),
        spending("travel", "travel", 8_000.0, "annual", true, Some(2026), Some(2036)),
    ];
    out.push(s);

    // Married couple in California with spouse-owned accounts and life events.
    let mut s = Scenario::new(
        "married_ca_life_events",
        "Married filing jointly in California with spouse-owned accounts, a home sale and an RV purchase.",
        with_spouse(profile(date(1963, 7, 4), 90, "married_filing_jointly", "CA"), date(1966, 11, 20), 94),
    );
    s.accounts = vec![
        account("joint_brokerage", "taxable", "brokerage", "joint", 500_000.0, 5.5, 2.0, Some(300_000.0)),
        account("p_401k", "tax_deferred", "401k", "self", 900_000.0, 6.0, 0.0, None),
        account("s_403b", "tax_deferred", "403b", "spouse", 400_000.0, 6.0, 0.0, None),
        account("s_roth", "tax_free", "roth_ira", "spouse", 150_000.0, 6.5, 0.0, None),
        account("hsa", "tax_free", "hsa", "self", 40_000.0, 5.0, 0.0, None),
    ];
    s.income = vec![
        social_security("ss_self", "self", 3_100.0, date(2030, 7, 1)),
        social_security("ss_spouse", "spouse", 1_900.0, date(2033, 11, 1)),
        income("consulting", "consulting", "self", 40_000.0, "annual", date(2026, 1, 1), Some(date(2028, 12, 31)), 3.0, false, "taxable"),
    ];
    s.spending = vec![
        living(96_000.0),
        spending("health", "healthcare", 18_000.0, "annual", true, None, None),
        spending("car", "vehicle_replacement", 45_000.0, "one_time", false, Some(2031), None),
        spending("roof", "home_maintenance", 30_000.0, "one_time", true, None, None),
    ];
    s.life_events = vec![
        event("sell_house", "sell_house", date(2032, 6, 1), "inflow", 450_000.0, true, true, "one_time", None),
        event("rv", "large_purchase", date(2033, 3, 1), "outflow", 120_000.0, false, false, "one_time", None),
    ];
    out.push(s);

    // Head of household in New York with a partially taxable annuity.
    let mut s = Scenario::new(
        "hoh_ny_annuity",
        "Head of household in New York with a partially taxable annuity and a tax-free muni income.",
        profile(date(1959, 1, 10), 88, "head_of_household", "NY"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 200_000.0, 5.0, 3.0, Some(120_000.0)),
        account("ira", "tax_deferred", "ira", "self", 300_000.0, 5.0, 0.0, None),
    ];
    s.income = vec![
        social_security("ss", "self", 2_100.0, date(2026, 1, 1)),
        income("annuity", "annuity", "self", 1_000.0, "monthly", date(2025, 1, 1), None, 0.0, false, "partially_taxable"),
        income("munis", "royalties", "self", 6_000.0, "annual", date(2020, 1, 1), Some(date(2035, 1, 1)), 0.0, false, "tax_free"),
    ];
    s.spending = vec![living(55_000.0)];
    out.push(s);

    // Married filing separately: Social Security taxed from the first dollar.
    let mut s = Scenario::new(
        "mfs_oregon",
        "Married filing separately in Oregon (flat 8.75%) with Social Security.",
        with_spouse(profile(date(1958, 5, 5), 90, "married_filing_separately", "OR"), date(1960, 2, 2), 90),
    );
    s.accounts = vec![
        account("ira", "tax_deferred", "ira", "self", 800_000.0, 5.5, 0.0, None),
        account("savings", "taxable", "savings", "self", 60_000.0, 3.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 2_400.0, date(2025, 5, 1))];
    s.spending = vec![living(60_000.0)];
    out.push(s);

    // Qualifying widow(er) uses the joint schedules.
    let mut s = Scenario::new(
        "qualifying_widow_ca",
        "Qualifying widow(er) in California: joint federal and state schedules.",
        profile(date(1964, 9, 30), 91, "qualifying_widow", "CA"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 400_000.0, 6.0, 1.5, Some(400_000.0)),
        account("ira", "tax_deferred", "ira", "self", 500_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 2_000.0, date(2031, 9, 1))];
    s.spending = vec![living(70_000.0)];
    out.push(s);

    // Pre-Medicare couple using the ACA subsidy.
    let mut s = Scenario::new(
        "aca_pre_medicare_couple",
        "Early-retired couple on ACA marketplace coverage until Medicare.",
        with_spouse(profile(date(1966, 4, 12), 90, "married_filing_jointly", "CO"), date(1968, 8, 8), 92),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "joint", 450_000.0, 6.0, 1.5, Some(380_000.0)),
        account("ira", "tax_deferred", "ira", "self", 700_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "spouse", 90_000.0, 6.5, 0.0, None),
    ];
    s.income = vec![
        income("pension", "pension", "self", 2_000.0, "monthly", date(2026, 1, 1), None, 1.0, false, "taxable"),
        social_security("ss", "self", 2_800.0, date(2033, 4, 1)),
    ];
    s.spending = vec![living(62_000.0), spending("health", "healthcare", 4_000.0, "annual", true, None, None)];
    s.assumptions.aca_benchmark_annual_premium = 19_500.0;
    out.push(s);

    // ACA plus Roth conversions: the subsidy/conversion tradeoff.
    let mut s = Scenario::new(
        "aca_with_roth_conversion",
        "Single 61-year-old with a fixed-ceiling Roth conversion shrinking the ACA subsidy.",
        profile(date(1965, 2, 2), 92, "single", "TX"),
    );
    s.accounts = vec![
        account("cash", "taxable", "money_market", "self", 150_000.0, 3.5, 0.0, Some(150_000.0)),
        account("ira", "tax_deferred", "ira", "self", 900_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 20_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![income("pension", "pension", "self", 24_000.0, "annual", date(2020, 1, 1), None, 0.0, false, "taxable")];
    s.spending = vec![living(40_000.0)];
    s.assumptions.aca_benchmark_annual_premium = 11_000.0;
    s.assumptions.roth_conversion_ceiling = 60_000.0;
    s.assumptions.roth_conversion_start_year = Some(2026);
    s.assumptions.roth_conversion_end_year = Some(2031);
    out.push(s);

    // High-income Medicare retiree paying IRMAA.
    let mut s = Scenario::new(
        "irmaa_high_income",
        "Single retiree past 65 with a large pension in an IRMAA tier.",
        profile(date(1955, 12, 1), 90, "single", "IL"),
    );
    s.accounts = vec![account("ira", "tax_deferred", "ira", "self", 1_500_000.0, 5.0, 0.0, None)];
    s.income = vec![
        income("pension", "pension", "self", 140_000.0, "annual", date(2015, 1, 1), None, 2.0, false, "taxable"),
        social_security("ss", "self", 3_800.0, date(2025, 12, 1)),
    ];
    s.spending = vec![living(110_000.0)];
    out.push(s);

    // Large RMDs for an older retiree.
    let mut s = Scenario::new(
        "rmd_heavy",
        "Born 1950 with a $2.4M IRA: RMDs exceed spending and are reinvested.",
        profile(date(1950, 6, 15), 98, "single", "FL"),
    );
    s.accounts = vec![
        account("ira", "tax_deferred", "ira", "self", 2_400_000.0, 5.0, 0.0, None),
        account("brokerage", "taxable", "brokerage", "self", 100_000.0, 5.0, 2.0, Some(100_000.0)),
    ];
    s.income = vec![social_security("ss", "self", 3_200.0, date(2016, 6, 1))];
    s.spending = vec![living(50_000.0)];
    out.push(s);

    // RMDs attributed per owner: joint-titled deferred money is the primary's.
    let mut s = Scenario::new(
        "rmd_spouse_ownership",
        "Primary past RMD age, younger spouse not; joint-tagged IRA counts as the primary's.",
        with_spouse(profile(date(1951, 1, 1), 92, "married_filing_jointly", "PA"), date(1962, 3, 3), 95),
    );
    s.accounts = vec![
        account("p_ira", "tax_deferred", "ira", "self", 700_000.0, 5.0, 0.0, None),
        account("joint_ira", "tax_deferred", "ira", "joint", 200_000.0, 5.0, 0.0, None),
        account("s_ira", "tax_deferred", "ira", "spouse", 600_000.0, 5.0, 0.0, None),
        account("s_roth", "tax_free", "roth_ira", "spouse", 100_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![
        social_security("ss_self", "self", 2_900.0, date(2021, 1, 1)),
        social_security("ss_spouse", "spouse", 1_500.0, date(2029, 3, 1)),
    ];
    s.spending = vec![living(85_000.0)];
    out.push(s);

    // Fixed-ceiling conversions inside a year window.
    let mut s = Scenario::new(
        "roth_fixed_ceiling_window",
        "Fixed-ceiling Roth conversions between 2027 and 2034.",
        profile(date(1962, 10, 10), 92, "single", "AZ"),
    );
    s.accounts = vec![
        account("cash", "taxable", "savings", "self", 250_000.0, 3.0, 0.0, Some(250_000.0)),
        account("ira", "tax_deferred", "ira", "self", 1_200_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 0.0, 6.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 2_700.0, date(2032, 10, 1))];
    s.spending = vec![living(52_000.0)];
    s.assumptions.roth_conversion_ceiling = 96_950.0;
    s.assumptions.roth_conversion_start_year = Some(2027);
    s.assumptions.roth_conversion_end_year = Some(2034);
    out.push(s);

    // IRMAA-aware conversions tapering as RMDs grow.
    let mut s = Scenario::new(
        "roth_irmaa_aware",
        "IRMAA-aware Roth conversions for a couple approaching RMD age.",
        with_spouse(profile(date(1956, 8, 1), 93, "married_filing_jointly", "NC"), date(1957, 9, 1), 93),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "joint", 600_000.0, 5.5, 1.5, Some(450_000.0)),
        account("ira", "tax_deferred", "ira", "self", 2_000_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 50_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![
        social_security("ss_self", "self", 3_400.0, date(2026, 8, 1)),
        social_security("ss_spouse", "spouse", 2_000.0, date(2027, 9, 1)),
    ];
    s.spending = vec![living(90_000.0)];
    s.assumptions.roth_conversion_mode = "irmaa_aware";
    out.push(s);

    // IRMAA-aware with a ceiling cap.
    let mut s = Scenario::new(
        "roth_irmaa_aware_with_ceiling",
        "IRMAA-aware conversions additionally capped by a flat ceiling.",
        profile(date(1960, 3, 3), 90, "single", "VA"),
    );
    s.accounts = vec![
        account("cash", "taxable", "savings", "self", 300_000.0, 3.0, 0.0, Some(300_000.0)),
        account("ira", "tax_deferred", "ira", "self", 1_800_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 10_000.0, 6.0, 0.0, None),
    ];
    s.spending = vec![living(45_000.0)];
    s.assumptions.roth_conversion_mode = "irmaa_aware";
    s.assumptions.roth_conversion_ceiling = 70_000.0;
    out.push(s);

    // Tax-optimized sequencing across accounts with different embedded gains.
    let mut s = Scenario::new(
        "tax_optimized_mixed_basis",
        "Tax-optimized sequencing with low- and high-gain taxable accounts and a pension near the 12%/15% boundary.",
        profile(date(1965, 1, 1), 92, "single", "TX"),
    );
    s.accounts = vec![
        account("cheap", "taxable", "brokerage", "self", 150_000.0, 6.0, 1.0, Some(135_000.0)),
        account("pricey", "taxable", "brokerage", "self", 400_000.0, 6.0, 1.0, Some(40_000.0)),
        account("ira", "tax_deferred", "ira", "self", 500_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 80_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![income("pension", "pension", "self", 63_400.0, "annual", date(2020, 1, 1), None, 0.0, false, "taxable")];
    s.spending = vec![living(100_000.0)];
    s.assumptions.withdrawal_strategy = "tax_optimized";
    out.push(s);

    // Maximize-income bracket filling.
    let mut s = Scenario::new(
        "maximize_income_bracket_fill",
        "Maximize-income search on a large IRA with years of low income before RMDs.",
        profile(date(1958, 4, 4), 100, "single", "TX"),
    );
    s.accounts = vec![
        account("ira", "tax_deferred", "ira", "self", 3_000_000.0, 5.0, 0.0, None),
        account("brokerage", "taxable", "brokerage", "self", 50_000.0, 5.0, 1.0, Some(50_000.0)),
    ];
    s.income = vec![social_security("ss", "self", 2_500.0, date(2028, 4, 1))];
    s.spending = vec![living(40_000.0)];
    s.assumptions.withdrawal_strategy = "maximize_income";
    out.push(s);

    // Maximize-income with only a Roth as the alternate destination.
    let mut s = Scenario::new(
        "maximize_income_roth_destination",
        "Maximize-income where the only non-deferred destination is a Roth.",
        profile(date(1958, 4, 4), 100, "single", "TX"),
    );
    s.accounts = vec![
        account("ira", "tax_deferred", "ira", "self", 3_000_000.0, 0.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 10_000.0, 0.0, 0.0, None),
    ];
    s.spending = vec![spending("living", "essential", 10_000.0, "annual", false, None, None)];
    s.assumptions.withdrawal_strategy = "maximize_income";
    out.push(s);

    // Maximize-income with nothing but tax-deferred money is a no-op.
    let mut s = Scenario::new(
        "maximize_income_deferred_only",
        "Maximize-income with only a tax-deferred account behaves like tax-optimized.",
        profile(date(1958, 4, 4), 100, "single", "TX"),
    );
    s.accounts = vec![account("ira", "tax_deferred", "ira", "self", 3_000_000.0, 4.0, 0.0, None)];
    s.spending = vec![living(30_000.0)];
    s.assumptions.withdrawal_strategy = "maximize_income";
    out.push(s);

    // Early retiree avoiding the 10% penalty.
    let mut s = Scenario::new(
        "avoid_early_withdrawal_penalty",
        "Retired at 51 with the penalty-avoidance flag: IRAs untouched until 59½.",
        with_spouse(profile(date(1975, 6, 1), 90, "married_filing_jointly", "WA"), date(1977, 1, 15), 90),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "joint", 300_000.0, 6.0, 1.5, Some(200_000.0)),
        account("p_401k", "tax_deferred", "401k", "self", 900_000.0, 6.0, 0.0, None),
        account("s_ira", "tax_deferred", "ira", "spouse", 400_000.0, 6.0, 0.0, None),
    ];
    s.spending = vec![living(80_000.0)];
    s.assumptions.avoid_early_withdrawal_penalty = true;
    out.push(s);

    // The same early retiree paying the penalty.
    let mut s = Scenario::new(
        "early_withdrawal_penalty_paid",
        "Retired at 51 without the flag: pre-59½ IRA draws pay the 10% penalty.",
        profile(date(1975, 6, 1), 90, "single", "MI"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 50_000.0, 6.0, 1.5, Some(40_000.0)),
        account("ira", "tax_deferred", "ira", "self", 1_100_000.0, 6.0, 0.0, None),
    ];
    s.spending = vec![living(75_000.0)];
    out.push(s);

    // Running out of money.
    let mut s = Scenario::new(
        "depletion",
        "Spending far beyond resources: depletion year and shortfalls.",
        profile(date(1960, 1, 1), 95, "single", "GA"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 150_000.0, 4.0, 1.0, Some(100_000.0)),
        account("ira", "tax_deferred", "ira", "self", 250_000.0, 4.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 1_800.0, date(2027, 1, 1))];
    s.spending = vec![living(70_000.0)];
    out.push(s);

    // Recurring and inflation-adjusted life events.
    let mut s = Scenario::new(
        "recurring_life_events",
        "Monthly and annual recurring life events with end dates, inflation, and taxable inflows.",
        with_spouse(profile(date(1962, 2, 28), 90, "married_filing_jointly", "MA"), date(1964, 2, 29), 90),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "joint", 400_000.0, 5.0, 1.5, Some(300_000.0)),
        account("ira", "tax_deferred", "ira", "self", 600_000.0, 5.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 100_000.0, 5.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 2_600.0, date(2029, 2, 1))];
    s.spending = vec![living(70_000.0)];
    s.life_events = vec![
        event("rent", "other", date(2027, 1, 1), "inflow", 1_800.0, true, true, "monthly", Some(date(2040, 12, 31))),
        event("care", "other", date(2030, 1, 1), "outflow", 12_000.0, false, true, "annual", Some(date(2035, 1, 1))),
        event("gift", "gift", date(2028, 5, 1), "outflow", 25_000.0, false, false, "one_time", None),
        event("inheritance", "inheritance", date(2034, 9, 1), "inflow", 300_000.0, false, false, "one_time", None),
    ];
    out.push(s);

    // Spending and income shapes: one-time, monthly, start/end windows, growth.
    let mut s = Scenario::new(
        "spending_and_income_shapes",
        "One-time, monthly, and windowed spending; income with growth, COLA, and end dates.",
        profile(date(1963, 12, 31), 90, "single", "NJ"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 500_000.0, 5.5, 1.2, Some(350_000.0)),
        account("ira", "tax_deferred", "ira", "self", 400_000.0, 5.5, 0.0, None),
    ];
    s.income = vec![
        social_security("ss", "self", 2_300.0, date(2030, 1, 1)),
        income("part_time", "part_time", "self", 2_500.0, "monthly", date(2026, 1, 1), Some(date(2029, 6, 30)), 2.5, false, "taxable"),
        income("rental", "rental", "self", 18_000.0, "annual", date(2024, 1, 1), None, 1.5, true, "taxable"),
    ];
    s.spending = vec![
        spending("living", "essential", 4_200.0, "monthly", true, None, None),
        spending("fun", "discretionary", 12_000.0, "annual", true, Some(2026), Some(2035)),
        spending("charity", "charity", 5_000.0, "annual", false, Some(2030), None),
        spending("kitchen", "large_purchase", 40_000.0, "one_time", false, Some(2029), None),
        spending("now", "one_time", 7_500.0, "one_time", true, None, None),
        spending("meds", "healthcare", 300.0, "monthly", true, Some(2028), None),
    ];
    out.push(s);

    // Deterministic per-year return shocks.
    let mut s = Scenario::new(
        "return_shocks",
        "A fixed sequence of per-year return shocks applied to every account.",
        profile(date(1961, 3, 15), 92, "single", "TX"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 400_000.0, 6.0, 1.5, Some(250_000.0)),
        account("ira", "tax_deferred", "ira", "self", 700_000.0, 6.0, 0.0, None),
        account("roth", "tax_free", "roth_ira", "self", 100_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 2_650.0, date(2028, 3, 1))];
    s.spending = vec![living(60_000.0)];
    s.shocks = Some(vec![-22.0, 15.5, -8.25, 4.0, 11.0, -3.5, 0.0, 19.0, -12.0, 6.5]);
    out.push(s);

    // A single market crash in year one (the what-if shape).
    let mut s = Scenario::new(
        "market_crash",
        "A one-time 35-point crash in the first projected year.",
        profile(date(1962, 1, 1), 90, "single", "OH"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 800_000.0, 6.0, 1.5, Some(500_000.0)),
        account("ira", "tax_deferred", "ira", "self", 600_000.0, 6.0, 0.0, None),
    ];
    s.spending = vec![living(65_000.0)];
    s.shocks = Some(vec![-35.0]);
    out.push(s);

    // Delayed Medicare enrollment.
    let mut s = Scenario::new(
        "delayed_medicare_enrollment",
        "Medicare enrollment delayed to 70: Part B and IRMAA counting start late.",
        profile(date(1959, 6, 1), 88, "single", "MN"),
    );
    s.accounts = vec![account("ira", "tax_deferred", "ira", "self", 900_000.0, 5.0, 0.0, None)];
    s.income = vec![income("salary", "employment", "self", 160_000.0, "annual", date(2010, 1, 1), Some(date(2029, 12, 31)), 3.0, false, "taxable")];
    s.spending = vec![living(70_000.0)];
    s.assumptions.medicare_enrollment_age = 70;
    out.push(s);

    // Medicare modeling disabled, high healthcare inflation, zero inflation.
    let mut s = Scenario::new(
        "medicare_disabled_custom_rates",
        "Part B disabled, 0% general inflation, 7% healthcare inflation, 3% SS COLA.",
        profile(date(1957, 7, 7), 89, "single", "KS"),
    );
    s.accounts = vec![account("brokerage", "taxable", "brokerage", "self", 900_000.0, 4.5, 2.5, Some(600_000.0))];
    s.income = vec![social_security("ss", "self", 3_000.0, date(2024, 7, 1))];
    s.spending = vec![living(50_000.0), spending("health", "healthcare", 9_000.0, "annual", true, None, None)];
    s.assumptions.medicare_part_b_annual_premium = 0.0;
    s.assumptions.inflation_rate = 0.0;
    s.assumptions.healthcare_inflation_rate = 7.0;
    s.assumptions.social_security_cola_rate = 3.0;
    out.push(s);

    // "Other" accounts are tracked but never drawn down.
    let mut s = Scenario::new(
        "other_accounts_excluded",
        "Pension and cash-value life accounts grow but are never withdrawn from.",
        profile(date(1960, 10, 1), 90, "single", "WI"),
    );
    s.accounts = vec![
        account("pension_acct", "other", "pension", "self", 300_000.0, 4.0, 0.0, None),
        account("life", "other", "cash_value_life_insurance", "self", 80_000.0, 3.0, 0.0, None),
        account("brokerage", "taxable", "brokerage", "self", 150_000.0, 5.0, 2.0, Some(150_000.0)),
    ];
    s.income = vec![social_security("ss", "self", 2_200.0, date(2027, 10, 1))];
    s.spending = vec![living(45_000.0)];
    out.push(s);

    // Default assumptions flag.
    let mut s = Scenario::new(
        "default_assumptions",
        "No saved assumptions: built-in defaults with the is_default flag.",
        profile(date(1962, 5, 20), 90, "single", "UT"),
    );
    s.accounts = vec![account("ira", "tax_deferred", "ira", "self", 750_000.0, 6.0, 0.0, None)];
    s.spending = vec![living(42_000.0)];
    s.assumptions_are_default = true;
    out.push(s);

    // Edge: no accounts at all.
    let mut s = Scenario::new(
        "no_accounts",
        "Income and spending but no accounts: every gap is a shortfall.",
        profile(date(1960, 1, 1), 85, "single", "SC"),
    );
    s.income = vec![social_security("ss", "self", 2_000.0, date(2026, 1, 1))];
    s.spending = vec![living(30_000.0)];
    out.push(s);

    // Edge: life expectancy already passed — a single projected year.
    let mut s = Scenario::new(
        "horizon_already_passed",
        "Life expectancy before the current year: projects only the current year.",
        profile(date(1940, 1, 1), 80, "single", "TX"),
    );
    s.accounts = vec![account("ira", "tax_deferred", "ira", "self", 300_000.0, 4.0, 0.0, None)];
    s.spending = vec![living(30_000.0)];
    out.push(s);

    // Seniors deduction: spouse only counts on a joint return.
    let mut s = Scenario::new(
        "seniors_deduction_joint",
        "Both spouses 65+ on a joint return: two senior add-ons.",
        with_spouse(profile(date(1958, 2, 2), 90, "married_filing_jointly", "ME"), date(1959, 3, 3), 90),
    );
    s.accounts = vec![account("ira", "tax_deferred", "ira", "self", 1_000_000.0, 5.0, 0.0, None)];
    s.income = vec![social_security("ss", "self", 3_000.0, date(2025, 2, 1))];
    s.spending = vec![living(75_000.0)];
    out.push(s);

    out
}

// ---- Writers -----------------------------------------------------------------

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    let mut text = serde_json::to_string_pretty(value).expect("serialize fixture");
    text.push('\n');
    fs::write(path, text).expect("write fixture");
}

fn projection_fixture(s: &Scenario) -> Value {
    let inputs = s.inputs();
    let response = match &s.shocks {
        Some(shocks) => run_projection_with_shocks(&inputs, shocks),
        None => run_projection(&inputs),
    };

    // Derived tasks and insights for the first projected year, so string
    // formatting is compared too.
    let first_year = response.annual.first();
    let tasks = derive_tasks(&TaskContext {
        current_year: CURRENT_YEAR,
        current_year_projection: first_year,
        current_quarter_projection: response.quarterly.first(),
    });
    let insights = generate_insights(&InsightContext {
        current_year: CURRENT_YEAR,
        primary_age: CURRENT_YEAR - s.profile.date_of_birth.year(),
        retirement_date: s.profile.retirement_date,
        current_year_projection: first_year,
        next_year_projection: response.annual.get(1),
        accounts: &s.accounts,
        review_due: true,
        latest_review: Some(ReviewVariance {
            label: "2026 Q1".into(),
            planned_spending: 12_500.0,
            spending_variance: -2_345.67,
            net_cash_flow: -150.25,
        }),
        monte_carlo_success_rate: Some(0.62),
    });

    json!({
        "name": s.name,
        "description": s.description,
        "inputs": {
            "current_year": CURRENT_YEAR,
            "profile": profile_json(&s.profile),
            "accounts": s.accounts.iter().map(account_json).collect::<Vec<_>>(),
            "income": s.income.iter().map(income_json).collect::<Vec<_>>(),
            "spending": s.spending.iter().map(spending_json).collect::<Vec<_>>(),
            "life_events": s.life_events.iter().map(life_event_json).collect::<Vec<_>>(),
            "assumptions": assumptions_json(&s.assumptions),
            "assumptions_are_default": s.assumptions_are_default,
            "return_shocks": s.shocks,
        },
        "expected": serde_json::to_value(&response).expect("serialize projection"),
        "expected_tasks": serde_json::to_value(&tasks).expect("serialize tasks"),
        "expected_insights": serde_json::to_value(&insights).expect("serialize insights"),
    })
}

fn tax_grid() -> Value {
    let tables = TaxTables::default_2025();
    let statuses = [
        FilingStatusKind::Single,
        FilingStatusKind::MarriedFilingJointly,
        FilingStatusKind::MarriedFilingSeparately,
        FilingStatusKind::HeadOfHousehold,
        FilingStatusKind::QualifyingWidow,
    ];
    let states = ["TX", "CA", "ca", "CO", "NY", "OR", "ZZ"];
    // (ordinary, qualified dividends, LTCG, Social Security, penalty)
    let incomes: [(f64, f64, f64, f64, f64); 9] = [
        (0.0, 0.0, 0.0, 0.0, 0.0),
        (12_000.0, 0.0, 0.0, 0.0, 0.0),
        (65_000.0, 0.0, 0.0, 0.0, 0.0),
        (45_000.0, 5_000.0, 40_000.0, 0.0, 0.0),
        (30_000.0, 0.0, 0.0, 24_000.0, 0.0),
        (90_000.0, 12_000.0, 25_000.0, 42_000.0, 0.0),
        (415_000.0, 30_000.0, 150_000.0, 50_000.0, 0.0),
        (1_250_000.0, 80_000.0, 600_000.0, 0.0, 0.0),
        (70_000.0, 0.0, 0.0, 0.0, 4_500.0),
    ];
    let years: [(i32, f64); 3] = [(2025, 0.0), (2040, 3.0), (2061, 2.5)];

    let mut cases = Vec::new();
    for &fs in &statuses {
        for state in states {
            for &(ordinary, qd, ltcg, ss, penalty) in &incomes {
                for &(year, inflation) in &years {
                    for seniors in [0u8, 2] {
                        let out = compute_taxes(
                            &TaxInput {
                                year,
                                filing_status: fs,
                                inflation_rate: inflation,
                                seniors_65_plus: seniors,
                                ordinary_income: ordinary,
                                qualified_dividends: qd,
                                long_term_capital_gains: ltcg,
                                social_security_benefits: ss,
                                state,
                                early_withdrawal_penalty: penalty,
                            },
                            &tables,
                        );
                        cases.push(json!({
                            "input": {
                                "year": year,
                                "filing_status": filing_str(fs),
                                "inflation_rate": inflation,
                                "seniors_65_plus": seniors,
                                "ordinary_income": ordinary,
                                "qualified_dividends": qd,
                                "long_term_capital_gains": ltcg,
                                "social_security_benefits": ss,
                                "state": state,
                                "early_withdrawal_penalty": penalty,
                            },
                            "expected": {
                                "taxable_social_security": out.taxable_social_security,
                                "adjusted_gross_income": out.adjusted_gross_income,
                                "standard_deduction": out.standard_deduction,
                                "taxable_income": out.taxable_income,
                                "federal_ordinary_tax": out.federal_ordinary_tax,
                                "federal_capital_gains_tax": out.federal_capital_gains_tax,
                                "early_withdrawal_penalty": out.early_withdrawal_penalty,
                                "federal_tax": out.federal_tax,
                                "state_taxable_income": out.state_taxable_income,
                                "state_standard_deduction": out.state_standard_deduction,
                                "state_tax": out.state_tax,
                                "state_marginal_rate": out.state_marginal_rate,
                                "total_tax": out.total_tax,
                                "effective_rate": out.effective_rate,
                                "marginal_rate": out.marginal_rate,
                            },
                        }));
                    }
                }
            }
        }
    }

    let mut marginal = Vec::new();
    for &fs in &statuses {
        for &(year, inflation) in &years {
            for ordinary_taxable in [0.0, 48_400.0, 100_000.0, 550_000.0, 700_000.0] {
                marginal.push(json!({
                    "filing_status": filing_str(fs),
                    "year": year,
                    "inflation_rate": inflation,
                    "ordinary_taxable_income": ordinary_taxable,
                    "expected": tables.capital_gains_marginal_rate(fs, year, inflation, ordinary_taxable),
                }));
            }
            for bracket_index in 0..8usize {
                marginal.push(json!({
                    "filing_status": filing_str(fs),
                    "year": year,
                    "inflation_rate": inflation,
                    "bracket_index": bracket_index,
                    "expected_bracket_top": tables.ordinary_bracket_top(fs, year, inflation, bracket_index),
                }));
            }
        }
    }

    json!({ "cases": cases, "table_queries": marginal })
}

fn aca_grid() -> Value {
    let tables = AcaTables::default_2025();
    let mut cases = Vec::new();
    for household_size in [1, 2, 3, 8, 12] {
        for magi in [0.0, 14_000.0, 19_562.5, 35_212.5, 47_000.0, 60_000.0, 90_000.0, 150_000.0] {
            for &(year, inflation) in &[(2025, 0.0), (2030, 2.5), (2045, 3.0)] {
                for benchmark in [0.0, 9_000.0, 20_000.0] {
                    let out = compute_subsidy(
                        &AcaInput {
                            year,
                            inflation_rate: inflation,
                            magi,
                            household_size,
                            benchmark_annual_premium: benchmark,
                        },
                        &tables,
                    );
                    cases.push(json!({
                        "input": {
                            "year": year,
                            "inflation_rate": inflation,
                            "magi": magi,
                            "household_size": household_size,
                            "benchmark_annual_premium": benchmark,
                        },
                        "expected": {
                            "magi": out.magi,
                            "federal_poverty_line": out.federal_poverty_line,
                            "fpl_percent": out.fpl_percent,
                            "applicable_percentage": out.applicable_percentage,
                            "expected_contribution": out.expected_contribution,
                            "benchmark_premium": out.benchmark_premium,
                            "subsidy": out.subsidy,
                            "eligible": out.eligible,
                        },
                    }));
                }
            }
        }
    }
    json!({ "cases": cases })
}

fn irmaa_grid() -> Value {
    let tables = IrmaaTables::default_2025();
    let groups = [
        IrmaaFilingGroup::Single,
        IrmaaFilingGroup::MarriedFilingJointly,
        IrmaaFilingGroup::MarriedFilingSeparately,
    ];
    let magis = [None, Some(90_000.0), Some(106_000.0), Some(150_000.0), Some(212_000.0), Some(395_000.0), Some(500_000.0), Some(2_000_000.0)];
    let mut cases = Vec::new();
    let mut thresholds = Vec::new();
    for &group in &groups {
        for &(year, inflation) in &[(2025, 0.0), (2033, 2.5), (2045, 3.0)] {
            for &magi in &magis {
                let out = compute_irmaa(
                    &IrmaaInput {
                        year,
                        inflation_rate: inflation,
                        lookback_magi: magi,
                        filing_group: group,
                    },
                    &tables,
                );
                cases.push(json!({
                    "input": {
                        "year": year,
                        "inflation_rate": inflation,
                        "lookback_magi": magi,
                        "filing_group": group_str(group),
                    },
                    "expected": {
                        "lookback_magi": out.lookback_magi,
                        "has_lookback_data": out.has_lookback_data,
                        "part_b_surcharge_monthly": out.part_b_surcharge_monthly,
                        "part_d_surcharge_monthly": out.part_d_surcharge_monthly,
                        "applies": out.applies,
                    },
                }));
                if let Some(m) = magi {
                    thresholds.push(json!({
                        "filing_group": group_str(group),
                        "year": year,
                        "inflation_rate": inflation,
                        "magi": m,
                        "expected": tables.next_threshold(group, year, inflation, m),
                    }));
                }
            }
        }
    }
    json!({ "cases": cases, "next_thresholds": thresholds })
}

/// Monte Carlo can't match the Rust random stream, so this records summary
/// statistics from a large run for a statistical comparison.
fn monte_carlo_fixture() -> Value {
    let mut s = Scenario::new(
        "monte_carlo_moderate_risk",
        "A plan with a moderate failure probability under 12% volatility.",
        profile(date(1961, 3, 15), 95, "single", "TX"),
    );
    s.accounts = vec![
        account("brokerage", "taxable", "brokerage", "self", 300_000.0, 6.0, 1.5, Some(200_000.0)),
        account("ira", "tax_deferred", "ira", "self", 600_000.0, 6.0, 0.0, None),
    ];
    s.income = vec![social_security("ss", "self", 2_400.0, date(2028, 3, 1))];
    s.spending = vec![living(62_000.0)];

    let simulations = 4_000;
    let result = run_monte_carlo(&s.inputs(), simulations, 12.0);
    let mut fixture = projection_fixture(&s);
    fixture["monte_carlo"] = json!({
        "num_simulations": simulations,
        "volatility": 12.0,
        "expected": serde_json::to_value(&result).expect("serialize monte carlo"),
    });
    fixture
}

#[test]
#[ignore = "writes parity fixtures; run via `cargo xtask export-fixtures`"]
fn export_parity_fixtures() {
    let out_dir = PathBuf::from(
        std::env::var("FIXTURE_OUT_DIR").expect("set FIXTURE_OUT_DIR to the fixture output directory"),
    );

    let projection_dir = out_dir.join("projection");
    if projection_dir.exists() {
        fs::remove_dir_all(&projection_dir).expect("clear old projection fixtures");
    }
    let all = scenarios();
    for s in &all {
        write_json(&projection_dir.join(format!("{}.json", s.name)), &projection_fixture(s));
    }
    write_json(&out_dir.join("tax_grid.json"), &tax_grid());
    write_json(&out_dir.join("aca_grid.json"), &aca_grid());
    write_json(&out_dir.join("irmaa_grid.json"), &irmaa_grid());
    write_json(&out_dir.join("monte_carlo.json"), &monte_carlo_fixture());

    println!("wrote {} projection fixtures to {}", all.len(), out_dir.display());
}
