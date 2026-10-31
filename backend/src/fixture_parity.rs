//! Cross-engine parity check (test-only). The macOS app's golden fixtures
//! (`Tests/PlannerEngineTests/Fixtures/projection/*.json`) hold a scenario's
//! inputs and the projection it must produce. This replays them through this
//! engine and requires every number to match exactly.
//!
//! Ignored by default; run with the fixtures directory:
//!
//! ```text
//! SWIFT_FIXTURES_DIR=../../lifetime_income_planner_macos/Tests/PlannerEngineTests/Fixtures \
//!     cargo test fixture_parity -- --ignored --nocapture
//! ```
//!
//! Numbers must agree to 1e-9 (relative); set `SWIFT_FIXTURE_EXACT=1` to demand
//! bit-for-bit equality.
//!
//! `SWIFT_FIXTURE_ACA=built_in` runs the scenarios against the current ACA
//! rules ([`AcaTables::built_in`]) instead of the original single curve.

use chrono::{NaiveDate, NaiveDateTime};
use serde_json::Value;

use crate::aca::AcaTables;
use crate::irmaa::IrmaaTables;
use crate::models::{Account, IncomeSource, LifeEvent, Profile, SpendingItem};
use crate::projection::{run_projection, run_projection_with_shocks, ProjectionInputs};
use crate::tax::TaxTables;

fn ts() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()
}

fn s(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_else(|| panic!("missing string `{key}`")).to_string()
}

fn opt_s(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(str::to_string)
}

fn f(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or_else(|| panic!("missing number `{key}`"))
}

fn opt_f(v: &Value, key: &str) -> Option<f64> {
    v[key].as_f64()
}

fn opt_i(v: &Value, key: &str) -> Option<i32> {
    v[key].as_i64().map(|n| n as i32)
}

fn d(v: &Value, key: &str) -> NaiveDate {
    NaiveDate::parse_from_str(v[key].as_str().unwrap(), "%Y-%m-%d").unwrap()
}

fn opt_d(v: &Value, key: &str) -> Option<NaiveDate> {
    v[key].as_str().map(|t| NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap())
}

fn profile(v: &Value) -> Profile {
    Profile {
        id: "p".into(),
        user_id: "u".into(),
        first_name: s(v, "first_name"),
        last_name: s(v, "last_name"),
        date_of_birth: d(v, "date_of_birth"),
        marital_status: s(v, "marital_status"),
        filing_status: s(v, "filing_status"),
        state: s(v, "state"),
        retirement_date: d(v, "retirement_date"),
        life_expectancy: v["life_expectancy"].as_i64().unwrap() as i32,
        spouse_first_name: opt_s(v, "spouse_first_name"),
        spouse_last_name: opt_s(v, "spouse_last_name"),
        spouse_date_of_birth: opt_d(v, "spouse_date_of_birth"),
        spouse_life_expectancy: opt_i(v, "spouse_life_expectancy"),
        created_at: ts(),
        updated_at: ts(),
    }
}

fn account(v: &Value) -> Account {
    Account {
        id: s(v, "id"),
        user_id: "u".into(),
        name: s(v, "name"),
        category: s(v, "category"),
        account_type: s(v, "account_type"),
        owner: s(v, "owner"),
        current_balance: f(v, "current_balance"),
        expected_roi: f(v, "expected_roi"),
        dividend_yield: f(v, "dividend_yield"),
        cost_basis: opt_f(v, "cost_basis"),
        allocation_stock_pct: opt_i(v, "allocation_stock_pct"),
        allocation_bond_pct: opt_i(v, "allocation_bond_pct"),
        allocation_cash_pct: opt_i(v, "allocation_cash_pct"),
        withdrawal_restrictions: opt_s(v, "withdrawal_restrictions"),
        created_at: ts(),
        updated_at: ts(),
    }
}

fn income(v: &Value) -> IncomeSource {
    IncomeSource {
        id: s(v, "id"),
        user_id: "u".into(),
        name: s(v, "name"),
        income_type: s(v, "income_type"),
        owner: s(v, "owner"),
        amount: f(v, "amount"),
        frequency: s(v, "frequency"),
        start_date: d(v, "start_date"),
        end_date: opt_d(v, "end_date"),
        growth_rate: f(v, "growth_rate"),
        cola: v["cola"].as_bool().unwrap(),
        taxability: s(v, "taxability"),
        notes: opt_s(v, "notes"),
        created_at: ts(),
        updated_at: ts(),
    }
}

fn spending(v: &Value) -> SpendingItem {
    SpendingItem {
        id: s(v, "id"),
        user_id: "u".into(),
        name: s(v, "name"),
        category: s(v, "category"),
        amount: f(v, "amount"),
        frequency: s(v, "frequency"),
        inflation_adjusted: v["inflation_adjusted"].as_bool().unwrap(),
        start_year: opt_i(v, "start_year"),
        end_year: opt_i(v, "end_year"),
        notes: opt_s(v, "notes"),
        created_at: ts(),
        updated_at: ts(),
    }
}

fn life_event(v: &Value) -> LifeEvent {
    LifeEvent {
        id: s(v, "id"),
        user_id: "u".into(),
        name: s(v, "name"),
        event_type: s(v, "event_type"),
        event_date: d(v, "event_date"),
        direction: s(v, "direction"),
        amount: f(v, "amount"),
        taxable: v["taxable"].as_bool().unwrap(),
        inflation_adjusted: v["inflation_adjusted"].as_bool().unwrap(),
        recurrence: s(v, "recurrence"),
        end_date: opt_d(v, "end_date"),
        notes: opt_s(v, "notes"),
        created_at: ts(),
        updated_at: ts(),
    }
}


/// Runs one fixture scenario's `inputs` through the engine with the given ACA
/// tables (the other reference tables are the 2025 defaults).
fn run_scenario(inp: &Value, aca: AcaTables) -> crate::models::ProjectionResponse {
    let a = &inp["assumptions"];
    let profile = profile(&inp["profile"]);
    let accounts: Vec<Account> = inp["accounts"].as_array().unwrap().iter().map(account).collect();
    let incomes: Vec<IncomeSource> = inp["income"].as_array().unwrap().iter().map(income).collect();
    let spendings: Vec<SpendingItem> = inp["spending"].as_array().unwrap().iter().map(spending).collect();
    let events: Vec<LifeEvent> = inp["life_events"].as_array().unwrap().iter().map(life_event).collect();

    let inputs = ProjectionInputs {
        current_year: inp["current_year"].as_i64().unwrap() as i32,
        profile: &profile,
        accounts: &accounts,
        income: &incomes,
        spending: &spendings,
        life_events: &events,
        inflation_rate: f(a, "inflation_rate"),
        investment_return_rate: f(a, "investment_return_rate"),
        healthcare_inflation_rate: f(a, "healthcare_inflation_rate"),
        social_security_cola_rate: f(a, "social_security_cola_rate"),
        assumptions_are_default: inp["assumptions_are_default"].as_bool().unwrap(),
        roth_conversion_ceiling: f(a, "roth_conversion_ceiling"),
        roth_conversion_start_year: opt_i(a, "roth_conversion_start_year"),
        roth_conversion_end_year: opt_i(a, "roth_conversion_end_year"),
        withdrawal_strategy: s(a, "withdrawal_strategy"),
        aca_benchmark_annual_premium: f(a, "aca_benchmark_annual_premium"),
        medicare_part_b_annual_premium: f(a, "medicare_part_b_annual_premium"),
        avoid_early_withdrawal_penalty: a["avoid_early_withdrawal_penalty"].as_bool().unwrap(),
        medicare_enrollment_age: a["medicare_enrollment_age"].as_i64().unwrap() as i32,
        roth_conversion_mode: s(a, "roth_conversion_mode"),
        tax_tables: TaxTables::default_2025(),
        aca_tables: aca,
        irmaa_tables: IrmaaTables::default_2025(),
    };
    match inp["return_shocks"].as_array() {
        Some(shocks) => {
            let shocks: Vec<f64> = shocks.iter().map(|x| x.as_f64().unwrap()).collect();
            run_projection_with_shocks(&inputs, &shocks)
        }
        None => run_projection(&inputs),
    }
}

/// Compares `actual` with `expected` exactly, collecting up to `limit` paths
/// that differ.
fn diff(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>, limit: usize, exact: bool) {
    if out.len() >= limit {
        return;
    }
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (k, ev) in e {
                match a.get(k) {
                    Some(av) => diff(&format!("{path}.{k}"), ev, av, out, limit, exact),
                    None => out.push(format!("{path}.{k}: missing in actual")),
                }
            }
            for k in a.keys() {
                if !e.contains_key(k) {
                    out.push(format!("{path}.{k}: unexpected in actual"));
                }
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                out.push(format!("{path}: length {} vs {}", e.len(), a.len()));
                return;
            }
            for (i, (ev, av)) in e.iter().zip(a).enumerate() {
                diff(&format!("{path}[{i}]"), ev, av, out, limit, exact);
            }
        }
        (Value::Number(e), Value::Number(a)) => {
            let (ef, af) = (e.as_f64().unwrap(), a.as_f64().unwrap());
            // The fixtures come from another machine: allow the last-digit
            // differences floating point produces unless `exact`.
            let close = exact && ef == af || !exact && (ef - af).abs() <= 1e-9 * ef.abs().max(1.0);
            if !close {
                out.push(format!("{path}: expected {e}, got {a}"));
            }
        }
        _ => {
            if expected != actual {
                out.push(format!("{path}: expected {expected}, got {actual}"));
            }
        }
    }
}

#[test]
#[ignore = "needs SWIFT_FIXTURES_DIR; see the module docs"]
fn replays_the_swift_projection_fixtures() {
    let dir = std::env::var("SWIFT_FIXTURES_DIR").expect("set SWIFT_FIXTURES_DIR");
    let aca = match std::env::var("SWIFT_FIXTURE_ACA").as_deref() {
        Ok("built_in") => AcaTables::built_in(),
        _ => AcaTables::default_2025(),
    };

    let exact = std::env::var("SWIFT_FIXTURE_EXACT").is_ok();
    let mut files: Vec<_> = std::fs::read_dir(format!("{dir}/projection"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures found in {dir}/projection");

    let mut failures = Vec::new();
    for path in &files {
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let out = run_scenario(&raw["inputs"], aca.clone());
        let actual = serde_json::to_value(&out).unwrap();

        let mut mismatches = Vec::new();
        diff("$", &raw["expected"], &actual, &mut mismatches, 400, exact);
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if mismatches.is_empty() {
            println!("ok   {name}");
        } else {
            println!("FAIL {name}");
            for m in &mismatches {
                println!("       {m}");
            }
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "{} fixtures differ: {failures:?}", failures.len());
}

// ---- Fixtures for the current ACA rules and the ACA-aware Roth conversion ------

fn assumptions_json(extra: Value) -> Value {
    let mut a = serde_json::json!({
        "inflation_rate": 2.5,
        "investment_return_rate": 6.0,
        "healthcare_inflation_rate": 4.5,
        "social_security_cola_rate": 2.0,
        "roth_conversion_ceiling": 0.0,
        "roth_conversion_start_year": null,
        "roth_conversion_end_year": null,
        "withdrawal_strategy": "conventional",
        "aca_benchmark_annual_premium": 0.0,
        "medicare_part_b_annual_premium": 2220.0,
        "avoid_early_withdrawal_penalty": false,
        "medicare_enrollment_age": 65,
        "roth_conversion_mode": "fixed_ceiling"
    });
    for (k, v) in extra.as_object().unwrap() {
        a[k] = v.clone();
    }
    a
}

fn profile_json(dob: &str, filing: &str, spouse_dob: Option<&str>) -> Value {
    serde_json::json!({
        "first_name": "Pat", "last_name": "Planner", "date_of_birth": dob,
        "marital_status": if spouse_dob.is_some() { "married" } else { "single" },
        "filing_status": filing, "state": "TX", "retirement_date": "2026-01-01",
        "life_expectancy": 92,
        "spouse_first_name": spouse_dob.map(|_| "Sam"),
        "spouse_last_name": spouse_dob.map(|_| "Planner"),
        "spouse_date_of_birth": spouse_dob,
        "spouse_life_expectancy": spouse_dob.map(|_| 94)
    })
}

fn account_json(id: &str, category: &str, account_type: &str, balance: f64, roi: f64) -> Value {
    serde_json::json!({
        "id": id, "name": format!("{id} account"), "category": category,
        "account_type": account_type, "owner": "self", "current_balance": balance,
        "expected_roi": roi, "dividend_yield": 0.0, "cost_basis": null,
        "allocation_stock_pct": null, "allocation_bond_pct": null, "allocation_cash_pct": null,
        "withdrawal_restrictions": null
    })
}

fn income_json(id: &str, income_type: &str, amount: f64, frequency: &str, start: &str, cola: bool) -> Value {
    serde_json::json!({
        "id": id, "name": format!("{id} income"), "income_type": income_type, "owner": "self",
        "amount": amount, "frequency": frequency, "start_date": start, "end_date": null,
        "growth_rate": 0.0, "cola": cola, "taxability": "taxable", "notes": null
    })
}

fn spending_json(id: &str, amount: f64) -> Value {
    serde_json::json!({
        "id": id, "name": format!("{id} spending"), "category": "essential", "amount": amount,
        "frequency": "annual", "inflation_adjusted": true, "start_year": null, "end_year": null,
        "notes": null
    })
}

/// Writes the scenarios the current ACA rules and the ACA-aware conversion
/// change, with this engine's output, to `$SWIFT_FIXTURES_OUT/projection_rules/`.
/// Run by hand when the engine's ACA behavior changes:
///
/// ```text
/// SWIFT_FIXTURES_OUT=../../lifetime_income_planner_macos/Tests/PlannerEngineTests/Fixtures \
///     cargo test export_aca_rules_fixtures -- --ignored
/// ```
#[test]
#[ignore = "writes fixture files; needs SWIFT_FIXTURES_OUT"]
fn export_aca_rules_fixtures() {
    let out_dir = std::env::var("SWIFT_FIXTURES_OUT").expect("set SWIFT_FIXTURES_OUT");
    let dir = format!("{out_dir}/projection_rules");
    std::fs::create_dir_all(&dir).unwrap();

    let aca = |extra: Value| {
        let mut base = serde_json::json!({
            "aca_benchmark_annual_premium": 10_000.0, "inflation_rate": 2.5,
            "healthcare_inflation_rate": 4.5, "investment_return_rate": 5.0
        });
        for (k, v) in extra.as_object().unwrap() {
            base[k] = v.clone();
        }
        assumptions_json(base)
    };

    let scenarios: Vec<(&str, &str, Value)> = vec![
        (
            "aca_enhanced_ends_couple",
            "A couple with pension income above 400% of the poverty line: enhanced credits in 2025, none from 2026.",
            serde_json::json!({
                "current_year": 2025,
                "profile": profile_json("1964-03-10", "married_filing_jointly", Some("1966-08-22")),
                "accounts": [account_json("brk", "taxable", "brokerage", 600_000.0, 5.0),
                             account_json("ira", "tax_deferred", "ira", 400_000.0, 5.0)],
                "income": [income_json("pension", "pension", 88_000.0, "annual", "2025-01-01", false)],
                "spending": [spending_json("living", 70_000.0)],
                "life_events": [],
                "assumptions": aca(serde_json::json!({"aca_benchmark_annual_premium": 20_000.0})),
                "assumptions_are_default": false
            }),
        ),
        (
            "aca_aware_roth_taxable_funded",
            "ACA-aware conversions while spending comes from a taxable account: MAGI lands just under the cutoff.",
            serde_json::json!({
                "current_year": 2026,
                "profile": profile_json("1966-06-01", "single", None),
                "accounts": [account_json("brk", "taxable", "brokerage", 500_000.0, 5.0),
                             account_json("ira", "tax_deferred", "ira", 1_000_000.0, 5.0),
                             account_json("roth", "tax_free", "roth_ira", 10_000.0, 5.0)],
                "income": [], "spending": [spending_json("living", 30_000.0)], "life_events": [],
                "assumptions": aca(serde_json::json!({"roth_conversion_mode": "aca_aware"})),
                "assumptions_are_default": false
            }),
        ),
        (
            "aca_aware_roth_ira_funded",
            "ACA-aware conversions while the IRA also funds spending and tax: the conversion is trimmed to fit.",
            serde_json::json!({
                "current_year": 2026,
                "profile": profile_json("1966-06-01", "single", None),
                "accounts": [account_json("ira", "tax_deferred", "ira", 1_000_000.0, 5.0),
                             account_json("roth", "tax_free", "roth_ira", 10_000.0, 5.0)],
                "income": [], "spending": [spending_json("living", 30_000.0)], "life_events": [],
                "assumptions": aca(serde_json::json!({"roth_conversion_mode": "aca_aware"})),
                "assumptions_are_default": false
            }),
        ),
        (
            "aca_aware_roth_couple_into_medicare",
            "A couple converting under the cutoff until Medicare, then a flat ceiling once the ACA window closes.",
            serde_json::json!({
                "current_year": 2026,
                "profile": profile_json("1964-02-14", "married_filing_jointly", Some("1965-09-30")),
                "accounts": [account_json("brk", "taxable", "brokerage", 300_000.0, 5.0),
                             account_json("ira", "tax_deferred", "ira", 900_000.0, 5.0),
                             account_json("roth", "tax_free", "roth_ira", 50_000.0, 5.0)],
                "income": [income_json("ss", "social_security", 3_200.0, "monthly", "2034-03-01", true)],
                "spending": [spending_json("living", 80_000.0)], "life_events": [],
                "assumptions": aca(serde_json::json!({
                    "aca_benchmark_annual_premium": 22_000.0,
                    "roth_conversion_mode": "aca_aware", "roth_conversion_ceiling": 90_000.0
                })),
                "assumptions_are_default": false
            }),
        ),
        (
            "fixed_conversion_loses_the_subsidy",
            "A fixed-ceiling conversion that overshoots the ACA cutoff, for contrast with the ACA-aware mode.",
            serde_json::json!({
                "current_year": 2026,
                "profile": profile_json("1966-06-01", "single", None),
                "accounts": [account_json("brk", "taxable", "brokerage", 500_000.0, 5.0),
                             account_json("ira", "tax_deferred", "ira", 1_000_000.0, 5.0),
                             account_json("roth", "tax_free", "roth_ira", 10_000.0, 5.0)],
                "income": [], "spending": [spending_json("living", 30_000.0)], "life_events": [],
                "assumptions": aca(serde_json::json!({"roth_conversion_ceiling": 90_000.0})),
                "assumptions_are_default": false
            }),
        ),
    ];

    for (name, description, inputs) in scenarios {
        let out = run_scenario(&inputs, AcaTables::built_in());
        let doc = serde_json::json!({
            "name": name, "description": description, "inputs": inputs,
            "expected": serde_json::to_value(&out).unwrap()
        });
        std::fs::write(
            format!("{dir}/{name}.json"),
            serde_json::to_string_pretty(&doc).unwrap() + "\n",
        )
        .unwrap();
        println!("wrote {dir}/{name}.json");
    }
}
