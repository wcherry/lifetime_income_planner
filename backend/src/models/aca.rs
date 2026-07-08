//! Persistence for the ACA reference parameters (roadmap Phase 3, feature 1).
//! These rows back the [`crate::aca::AcaTables`] the subsidy engine reads. They
//! are seeded at startup from the built-in 2025 values and are intended to be
//! maintained through an admin role in a later phase.

use diesel::prelude::*;
use uuid::Uuid;

use crate::aca::{default_2025_inputs, AcaTables, ApplicablePercentageInput, FplInput};
use crate::schema::{aca_applicable_percentages, aca_fpl_guidelines};

/// A Federal Poverty Line guideline row. `id`/`base_year` are filtered on in the
/// query rather than read back.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = aca_fpl_guidelines)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AcaFplRow {
    pub household_size: i32,
    pub annual_amount: f64,
}

#[derive(Insertable)]
#[diesel(table_name = aca_fpl_guidelines)]
struct NewAcaFpl {
    id: String,
    base_year: i32,
    household_size: i32,
    annual_amount: f64,
}

/// An applicable-percentage breakpoint row.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = aca_applicable_percentages)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AcaApplicablePercentageRow {
    pub fpl_percent: f64,
    pub applicable_percentage: f64,
}

#[derive(Insertable)]
#[diesel(table_name = aca_applicable_percentages)]
struct NewAcaApplicablePercentage {
    id: String,
    fpl_percent: f64,
    applicable_percentage: f64,
}

/// Seed the ACA tables from the built-in 2025 values where they are empty. Safe
/// to call on every startup — it is a no-op for any group already populated, so
/// admin edits are preserved.
pub fn seed_aca_tables_if_empty(conn: &mut SqliteConnection) -> QueryResult<()> {
    let (year, fpl, applicable) = default_2025_inputs();

    let fpl_count: i64 = aca_fpl_guidelines::table.count().get_result(conn)?;
    if fpl_count == 0 {
        let rows: Vec<NewAcaFpl> = fpl
            .into_iter()
            .map(|f: FplInput| NewAcaFpl {
                id: Uuid::new_v4().to_string(),
                base_year: year,
                household_size: f.household_size,
                annual_amount: f.annual_amount,
            })
            .collect();
        diesel::insert_into(aca_fpl_guidelines::table)
            .values(&rows)
            .execute(conn)?;
    }

    let pct_count: i64 = aca_applicable_percentages::table.count().get_result(conn)?;
    if pct_count == 0 {
        let rows: Vec<NewAcaApplicablePercentage> = applicable
            .into_iter()
            .map(|p: ApplicablePercentageInput| NewAcaApplicablePercentage {
                id: Uuid::new_v4().to_string(),
                fpl_percent: p.fpl_percent,
                applicable_percentage: p.applicable_percentage,
            })
            .collect();
        diesel::insert_into(aca_applicable_percentages::table)
            .values(&rows)
            .execute(conn)?;
    }

    Ok(())
}

/// Load the ACA parameters into an in-memory [`AcaTables`]. Uses the most recent
/// published base year present in `aca_fpl_guidelines`. Falls back to the
/// built-in 2025 values if the tables are empty.
pub fn load_aca_tables(conn: &mut SqliteConnection) -> QueryResult<AcaTables> {
    use diesel::dsl::max;

    let base_year: Option<i32> = aca_fpl_guidelines::table
        .select(max(aca_fpl_guidelines::base_year))
        .first(conn)?;

    let Some(base_year) = base_year else {
        return Ok(AcaTables::default_2025());
    };

    let fpl_rows: Vec<AcaFplRow> = aca_fpl_guidelines::table
        .filter(aca_fpl_guidelines::base_year.eq(base_year))
        .select(AcaFplRow::as_select())
        .load(conn)?;
    let pct_rows: Vec<AcaApplicablePercentageRow> = aca_applicable_percentages::table
        .select(AcaApplicablePercentageRow::as_select())
        .load(conn)?;

    let fpl = fpl_rows
        .into_iter()
        .map(|r| FplInput {
            household_size: r.household_size,
            annual_amount: r.annual_amount,
        })
        .collect();
    let applicable = pct_rows
        .into_iter()
        .map(|r| ApplicablePercentageInput {
            fpl_percent: r.fpl_percent,
            applicable_percentage: r.applicable_percentage,
        })
        .collect();

    Ok(AcaTables::from_inputs(base_year, fpl, applicable))
}
