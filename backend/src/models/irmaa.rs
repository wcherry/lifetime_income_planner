//! Persistence for the Medicare IRMAA reference parameters (roadmap Phase 3,
//! feature 4). These rows back the [`crate::irmaa::IrmaaTables`] the surcharge
//! engine reads. They are seeded at startup from the built-in 2025 CMS values
//! and are intended to be maintained through an admin role in a later phase.

use diesel::prelude::*;
use uuid::Uuid;

use crate::irmaa::{default_2025_inputs, IrmaaBracketInput, IrmaaTables};
use crate::schema::irmaa_brackets;

/// A single IRMAA bracket row.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = irmaa_brackets)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct IrmaaBracketRow {
    pub filing_group: String,
    pub magi_threshold: f64,
    pub part_b_surcharge_monthly: f64,
    pub part_d_surcharge_monthly: f64,
}

#[derive(Insertable)]
#[diesel(table_name = irmaa_brackets)]
struct NewIrmaaBracket {
    id: String,
    base_year: i32,
    filing_group: String,
    magi_threshold: f64,
    part_b_surcharge_monthly: f64,
    part_d_surcharge_monthly: f64,
}

/// Seed the IRMAA brackets from the built-in 2025 CMS values where the table
/// is empty. Safe to call on every startup — it is a no-op once populated, so
/// admin edits are preserved.
pub fn seed_irmaa_brackets_if_empty(conn: &mut SqliteConnection) -> QueryResult<()> {
    let (year, rows) = default_2025_inputs();

    let count: i64 = irmaa_brackets::table.count().get_result(conn)?;
    if count == 0 {
        let new_rows: Vec<NewIrmaaBracket> = rows
            .into_iter()
            .map(|r: IrmaaBracketInput| NewIrmaaBracket {
                id: Uuid::new_v4().to_string(),
                base_year: year,
                filing_group: r.filing_group,
                magi_threshold: r.magi_threshold,
                part_b_surcharge_monthly: r.part_b_surcharge_monthly,
                part_d_surcharge_monthly: r.part_d_surcharge_monthly,
            })
            .collect();
        diesel::insert_into(irmaa_brackets::table)
            .values(&new_rows)
            .execute(conn)?;
    }

    Ok(())
}

/// Load the IRMAA parameters into an in-memory [`IrmaaTables`]. Uses the most
/// recent published base year present in `irmaa_brackets`. Falls back to the
/// built-in 2025 values if the table is empty.
pub fn load_irmaa_tables(conn: &mut SqliteConnection) -> QueryResult<IrmaaTables> {
    use diesel::dsl::max;

    let base_year: Option<i32> = irmaa_brackets::table
        .select(max(irmaa_brackets::base_year))
        .first(conn)?;

    let Some(base_year) = base_year else {
        return Ok(IrmaaTables::default_2025());
    };

    let rows: Vec<IrmaaBracketRow> = irmaa_brackets::table
        .filter(irmaa_brackets::base_year.eq(base_year))
        .select(IrmaaBracketRow::as_select())
        .load(conn)?;

    let inputs = rows
        .into_iter()
        .map(|r| IrmaaBracketInput {
            filing_group: r.filing_group,
            magi_threshold: r.magi_threshold,
            part_b_surcharge_monthly: r.part_b_surcharge_monthly,
            part_d_surcharge_monthly: r.part_d_surcharge_monthly,
        })
        .collect();

    Ok(IrmaaTables::from_inputs(base_year, inputs))
}
