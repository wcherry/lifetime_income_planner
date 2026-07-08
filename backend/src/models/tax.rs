//! Persistence for the reference tax parameters (roadmap Phase 2, features
//! 1–5). These rows back the [`crate::tax::TaxTables`] the engine reads. They
//! are seeded at startup from the built-in 2025 values and are intended to be
//! maintained through an admin role in a later phase.

use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::{state_tax_brackets, state_tax_params, tax_brackets, tax_filing_params};
use crate::tax::{
    default_2025_inputs, BracketInput, FilingParamInput, StateBracketInput, StateParamInput,
    TaxTables,
};

/// A federal bracket row (ordinary or capital-gains schedule). Only the columns
/// the engine consumes are selected; `id`/`tax_year` are filtered on in the
/// query rather than read back.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = tax_brackets)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct TaxBracketRow {
    pub bracket_type: String,
    pub filing_status: String,
    pub floor_amount: f64,
    pub rate: f64,
}

#[derive(Insertable)]
#[diesel(table_name = tax_brackets)]
struct NewTaxBracket {
    id: String,
    tax_year: i32,
    bracket_type: String,
    filing_status: String,
    floor_amount: f64,
    rate: f64,
}

/// A per-filing-status parameter row.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = tax_filing_params)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct TaxFilingParamRow {
    pub filing_status: String,
    pub standard_deduction: f64,
    pub additional_senior_deduction: f64,
    pub ss_base_threshold: f64,
    pub ss_second_threshold: f64,
}

#[derive(Insertable)]
#[diesel(table_name = tax_filing_params)]
struct NewTaxFilingParam {
    id: String,
    tax_year: i32,
    filing_status: String,
    standard_deduction: f64,
    additional_senior_deduction: f64,
    ss_base_threshold: f64,
    ss_second_threshold: f64,
}

/// A state bracket row.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = state_tax_brackets)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct StateTaxBracketRow {
    pub state: String,
    pub filing_status: String,
    pub floor_amount: f64,
    pub rate: f64,
}

#[derive(Insertable)]
#[diesel(table_name = state_tax_brackets)]
struct NewStateTaxBracket {
    id: String,
    state: String,
    filing_status: String,
    floor_amount: f64,
    rate: f64,
}

/// A per-state, per-filing-status parameter row. SQLite stores the booleans as
/// 0/1 integers.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = state_tax_params)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct StateTaxParamRow {
    pub state: String,
    pub filing_status: String,
    pub standard_deduction: f64,
    pub taxes_social_security: i32,
    pub taxes_capital_gains_as_ordinary: i32,
}

#[derive(Insertable)]
#[diesel(table_name = state_tax_params)]
struct NewStateTaxParam {
    id: String,
    state: String,
    filing_status: String,
    standard_deduction: f64,
    taxes_social_security: i32,
    taxes_capital_gains_as_ordinary: i32,
}

/// Seed the tax tables from the built-in 2025 values where they are empty. Safe
/// to call on every startup — it is a no-op for any group already populated, so
/// admin edits are preserved. The federal and state groups are seeded
/// independently so a database that already has the federal tables (but gained
/// the state tables in a later migration) is still filled in.
pub fn seed_tax_tables_if_empty(conn: &mut SqliteConnection) -> QueryResult<()> {
    let (year, brackets, params, state_brackets, state_params) = default_2025_inputs();

    let federal_empty: i64 = tax_brackets::table.count().get_result(conn)?;
    if federal_empty == 0 {
        let bracket_rows: Vec<NewTaxBracket> = brackets
            .into_iter()
            .map(|b: BracketInput| NewTaxBracket {
                id: Uuid::new_v4().to_string(),
                tax_year: year,
                bracket_type: b.bracket_type,
                filing_status: b.filing_status,
                floor_amount: b.floor,
                rate: b.rate,
            })
            .collect();
        let param_rows: Vec<NewTaxFilingParam> = params
            .into_iter()
            .map(|p: FilingParamInput| NewTaxFilingParam {
                id: Uuid::new_v4().to_string(),
                tax_year: year,
                filing_status: p.filing_status,
                standard_deduction: p.standard_deduction,
                additional_senior_deduction: p.additional_senior_deduction,
                ss_base_threshold: p.ss_base_threshold,
                ss_second_threshold: p.ss_second_threshold,
            })
            .collect();
        conn.transaction(|conn| {
            diesel::insert_into(tax_brackets::table)
                .values(&bracket_rows)
                .execute(conn)?;
            diesel::insert_into(tax_filing_params::table)
                .values(&param_rows)
                .execute(conn)?;
            QueryResult::Ok(())
        })?;
    }

    let state_empty: i64 = state_tax_brackets::table.count().get_result(conn)?;
    if state_empty == 0 {
        let state_bracket_rows: Vec<NewStateTaxBracket> = state_brackets
            .into_iter()
            .map(|b: StateBracketInput| NewStateTaxBracket {
                id: Uuid::new_v4().to_string(),
                state: b.state,
                filing_status: b.filing_status,
                floor_amount: b.floor,
                rate: b.rate,
            })
            .collect();
        let state_param_rows: Vec<NewStateTaxParam> = state_params
            .into_iter()
            .map(|p: StateParamInput| NewStateTaxParam {
                id: Uuid::new_v4().to_string(),
                state: p.state,
                filing_status: p.filing_status,
                standard_deduction: p.standard_deduction,
                taxes_social_security: p.taxes_social_security as i32,
                taxes_capital_gains_as_ordinary: p.taxes_capital_gains_as_ordinary as i32,
            })
            .collect();
        conn.transaction(|conn| {
            diesel::insert_into(state_tax_brackets::table)
                .values(&state_bracket_rows)
                .execute(conn)?;
            diesel::insert_into(state_tax_params::table)
                .values(&state_param_rows)
                .execute(conn)?;
            QueryResult::Ok(())
        })?;
    }

    Ok(())
}

/// Load the tax parameters into an in-memory [`TaxTables`]. Uses the most recent
/// published tax year present in `tax_brackets` as the base year. Falls back to
/// the built-in 2025 values if the tables are empty.
pub fn load_tax_tables(conn: &mut SqliteConnection) -> QueryResult<TaxTables> {
    use diesel::dsl::max;

    let base_year: Option<i32> = tax_brackets::table
        .select(max(tax_brackets::tax_year))
        .first(conn)?;

    let Some(base_year) = base_year else {
        return Ok(TaxTables::default_2025());
    };

    let bracket_rows: Vec<TaxBracketRow> = tax_brackets::table
        .filter(tax_brackets::tax_year.eq(base_year))
        .select(TaxBracketRow::as_select())
        .load(conn)?;
    let param_rows: Vec<TaxFilingParamRow> = tax_filing_params::table
        .filter(tax_filing_params::tax_year.eq(base_year))
        .select(TaxFilingParamRow::as_select())
        .load(conn)?;
    let state_bracket_rows: Vec<StateTaxBracketRow> = state_tax_brackets::table
        .select(StateTaxBracketRow::as_select())
        .load(conn)?;
    let state_param_rows: Vec<StateTaxParamRow> = state_tax_params::table
        .select(StateTaxParamRow::as_select())
        .load(conn)?;

    let brackets = bracket_rows
        .into_iter()
        .map(|r| BracketInput {
            bracket_type: r.bracket_type,
            filing_status: r.filing_status,
            floor: r.floor_amount,
            rate: r.rate,
        })
        .collect();
    let params = param_rows
        .into_iter()
        .map(|r| FilingParamInput {
            filing_status: r.filing_status,
            standard_deduction: r.standard_deduction,
            additional_senior_deduction: r.additional_senior_deduction,
            ss_base_threshold: r.ss_base_threshold,
            ss_second_threshold: r.ss_second_threshold,
        })
        .collect();
    let state_brackets = state_bracket_rows
        .into_iter()
        .map(|r| StateBracketInput {
            state: r.state,
            filing_status: r.filing_status,
            floor: r.floor_amount,
            rate: r.rate,
        })
        .collect();
    let state_params = state_param_rows
        .into_iter()
        .map(|r| StateParamInput {
            state: r.state,
            filing_status: r.filing_status,
            standard_deduction: r.standard_deduction,
            taxes_social_security: r.taxes_social_security != 0,
            taxes_capital_gains_as_ordinary: r.taxes_capital_gains_as_ordinary != 0,
        })
        .collect();

    Ok(TaxTables::from_inputs(
        base_year,
        brackets,
        params,
        state_brackets,
        state_params,
    ))
}
