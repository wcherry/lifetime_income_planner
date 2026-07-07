//! Projection engine (roadmap Phase 1, features 8 & 9).
//!
//! Given a user's profile, accounts, income, spending, life events, and
//! planning assumptions, this produces a year-by-year cash-flow projection and
//! a near-term quarterly withdrawal schedule. Phase 1 is deliberately
//! *pre-tax*: taxes are modeled as ordinary spending items, and tax-aware
//! sequencing arrives in Phase 2. The engine is pure (no I/O) so it can be unit
//! tested in isolation.

use std::collections::HashMap;

use chrono::{Datelike, Months, NaiveDate};

use crate::models::{
    Account, IncomeSource, LifeEvent, LifeEventOccurrence, Milestone, Profile,
    ProjectionAssumptions, ProjectionResponse, ProjectionSummary, QuarterProjection,
    QuarterWithdrawal, SpendingItem, YearProjection,
};

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
            },
        ));
    }
    out.push((
        by + 62,
        Milestone {
            label: "Social Security eligibility".into(),
            detail: format!("Earliest Social Security for {who} (age 62, reduced benefit)."),
            age: 62,
        },
    ));
    out.push((
        by + 65,
        Milestone {
            label: "Medicare eligibility".into(),
            detail: format!("Medicare eligibility begins for {who} (age 65)."),
            age: 65,
        },
    ));
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
            },
        ));
    }
    out.push((
        by + 70,
        Milestone {
            label: "Maximum Social Security".into(),
            detail: format!("Delayed retirement credits stop growing for {who} (age 70)."),
            age: 70,
        },
    ));
    let ra = rmd_age(by);
    out.push((
        by + ra,
        Milestone {
            label: "RMDs begin".into(),
            detail: format!("Required minimum distributions must begin for {who} (age {ra})."),
            age: ra,
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

    let mut milestones = build_milestones(inp.profile, start_year, end_year);

    // Running per-account balances, parallel to `inp.accounts`.
    let mut balances: Vec<f64> = inp.accounts.iter().map(|a| a.current_balance).collect();

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
    // Surplus is reinvested into the first drawdown account (taxable first).
    let reinvest_target = order.first().copied();

    let current_net_worth: f64 = balances.iter().sum();

    let mut annual: Vec<YearProjection> = Vec::new();
    let mut first_year_withdrawals: Vec<QuarterWithdrawal> = Vec::new();
    let mut total_income = 0.0;
    let mut total_spending = 0.0;
    let mut total_withdrawals = 0.0;
    let mut depletion_year: Option<i32> = None;

    for year in start_year..=end_year {
        let starting_balance: f64 = balances.iter().sum();

        let income: f64 = inp
            .income
            .iter()
            .map(|s| income_for_year(s, year, inp.social_security_cola_rate))
            .sum();
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
        let mut life_events_net = 0.0;
        let mut year_events: Vec<LifeEventOccurrence> = Vec::new();
        for e in inp.life_events {
            let amount = life_event_for_year(e, year, inp.current_year, inp.inflation_rate);
            if amount != 0.0 {
                life_events_net += amount;
                year_events.push(LifeEventOccurrence {
                    name: e.name.clone(),
                    amount: round2(amount),
                });
            }
        }

        // Credit a full year of growth before withdrawals (Phase 1 simplification).
        let mut growth = 0.0;
        for (i, acc) in inp.accounts.iter().enumerate() {
            let g = balances[i] * acc.expected_roi / 100.0;
            balances[i] += g;
            growth += g;
        }

        let net = income + life_events_net - spending;
        let mut withdrawals = 0.0;
        let mut contributions = 0.0;
        let mut shortfall = 0.0;

        if net < 0.0 {
            let mut need = -net;
            for &idx in &order {
                if need <= 0.0 {
                    break;
                }
                let take = balances[idx].min(need);
                if take <= 0.0 {
                    continue;
                }
                balances[idx] -= take;
                need -= take;
                withdrawals += take;
                if year == start_year {
                    let acc = &inp.accounts[idx];
                    first_year_withdrawals.push(QuarterWithdrawal {
                        account_id: acc.id.clone(),
                        account_name: acc.name.clone(),
                        category: acc.category.clone(),
                        amount: take,
                    });
                }
            }
            if need > 0.01 {
                shortfall = need;
                if depletion_year.is_none() {
                    depletion_year = Some(year);
                }
            }
        } else if net > 0.0 {
            if let Some(idx) = reinvest_target {
                balances[idx] += net;
                contributions = net;
            }
            // With no drawdown account to hold it, surplus simply goes unmodeled.
        }

        let ending_balance: f64 = balances.iter().sum();

        total_income += income;
        total_spending += spending;
        total_withdrawals += withdrawals;

        annual.push(YearProjection {
            year,
            primary_age: year - birth_year,
            spouse_age: spouse_birth_year.map(|b| year - b),
            starting_balance: round2(starting_balance),
            income: round2(income),
            spending: round2(spending),
            life_events_net: round2(life_events_net),
            life_events: year_events,
            milestones: milestones.remove(&year).unwrap_or_default(),
            growth: round2(growth),
            withdrawals: round2(withdrawals),
            contributions: round2(contributions),
            ending_balance: round2(ending_balance),
            shortfall: round2(shortfall),
        });
    }

    let projected_ending_balance = annual.last().map(|y| y.ending_balance).unwrap_or(0.0);
    let first = annual.first();
    let quarterly = build_quarterly(
        start_year,
        first.map(|y| y.income).unwrap_or(0.0),
        first.map(|y| y.spending).unwrap_or(0.0),
        &first_year_withdrawals,
    );

    ProjectionResponse {
        current_year: inp.current_year,
        start_year,
        end_year,
        assumptions: ProjectionAssumptions {
            inflation_rate: inp.inflation_rate,
            investment_return_rate: inp.investment_return_rate,
            healthcare_inflation_rate: inp.healthcare_inflation_rate,
            social_security_cola_rate: inp.social_security_cola_rate,
            is_default: inp.assumptions_are_default,
        },
        summary: ProjectionSummary {
            current_net_worth: round2(current_net_worth),
            projected_ending_balance,
            total_lifetime_income: round2(total_income),
            total_lifetime_spending: round2(total_spending),
            total_lifetime_withdrawals: round2(total_withdrawals),
            depletion_year,
        },
        annual,
        quarterly,
    }
}

/// Split the first projection year's totals evenly across four quarters to form
/// the actionable near-term withdrawal schedule (feature 9).
fn build_quarterly(
    year: i32,
    year_income: f64,
    year_spending: f64,
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
                total_withdrawal,
                withdrawals,
            }
        })
        .collect()
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
        let inc = [income("pension", 40_000.0, "annual", date(2020, 1, 1), false, 0.0)];
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
}
