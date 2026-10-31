//! Persistence for the ACA reference parameters (roadmap Phase 3, feature 1).
//! These rows back the [`crate::aca::AcaTables`] the subsidy engine reads. They
//! are seeded at startup from the built-in 2025 values and are intended to be
//! maintained through an admin role in a later phase.

use diesel::prelude::*;
use uuid::Uuid;

use crate::aca::{
    default_2025_inputs, AcaRules, AcaTables, ApplicablePercentageInput, FplInput,
};
use crate::schema::{
    aca_applicable_percentages, aca_fpl_guidelines, aca_rules, aca_standard_applicable_percentages,
};

/// The single `aca_rules` row: how the credit changes over time.
#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = aca_rules)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AcaRulesRow {
    pub id: String,
    pub enhanced_through_year: Option<i32>,
    pub cliff_fpl_percent: Option<f64>,
    pub guideline_lag_years: i32,
}

/// A row of the original (post-enhanced) applicable-percentage curve.
#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = aca_standard_applicable_percentages)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AcaStandardPercentageRow {
    pub id: String,
    pub sort_order: i32,
    pub fpl_percent: f64,
    pub applicable_percentage: f64,
}

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

    // The rules (enhanced credits ending, the cutoff, the guideline lag) and the
    // original curve they switch to.
    let rules = AcaRules::current();
    let rules_count: i64 = aca_rules::table.count().get_result(conn)?;
    if rules_count == 0 {
        diesel::insert_into(aca_rules::table)
            .values(&AcaRulesRow {
                id: Uuid::new_v4().to_string(),
                enhanced_through_year: rules.enhanced_through_year,
                cliff_fpl_percent: rules.cliff_fpl_percent,
                guideline_lag_years: rules.guideline_lag_years,
            })
            .execute(conn)?;
    }
    let standard_count: i64 = aca_standard_applicable_percentages::table
        .count()
        .get_result(conn)?;
    if standard_count == 0 {
        let rows: Vec<AcaStandardPercentageRow> = rules
            .standard_percentages
            .iter()
            .enumerate()
            .map(|(i, &(fpl_percent, applicable_percentage))| AcaStandardPercentageRow {
                id: Uuid::new_v4().to_string(),
                sort_order: i as i32,
                fpl_percent,
                applicable_percentage,
            })
            .collect();
        diesel::insert_into(aca_standard_applicable_percentages::table)
            .values(&rows)
            .execute(conn)?;
    }

    Ok(())
}

/// Load the ACA parameters into an in-memory [`AcaTables`]. Uses the most recent
/// published base year present in `aca_fpl_guidelines`. Falls back to the
/// built-in values if the tables are empty.
pub fn load_aca_tables(conn: &mut SqliteConnection) -> QueryResult<AcaTables> {
    use diesel::dsl::max;

    let base_year: Option<i32> = aca_fpl_guidelines::table
        .select(max(aca_fpl_guidelines::base_year))
        .first(conn)?;

    let Some(base_year) = base_year else {
        return Ok(AcaTables::built_in());
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

    // Without a rules row the original single-curve behavior applies.
    let rules_row: Option<AcaRulesRow> = aca_rules::table
        .select(AcaRulesRow::as_select())
        .first(conn)
        .optional()?;
    let rules = match rules_row {
        None => AcaRules::legacy(),
        Some(row) => {
            let standard: Vec<AcaStandardPercentageRow> = aca_standard_applicable_percentages::table
                .order((
                    aca_standard_applicable_percentages::fpl_percent.asc(),
                    aca_standard_applicable_percentages::sort_order.asc(),
                ))
                .select(AcaStandardPercentageRow::as_select())
                .load(conn)?;
            AcaRules {
                enhanced_through_year: row.enhanced_through_year,
                standard_percentages: standard
                    .into_iter()
                    .map(|r| (r.fpl_percent, r.applicable_percentage))
                    .collect(),
                cliff_fpl_percent: row.cliff_fpl_percent,
                guideline_lag_years: row.guideline_lag_years,
            }
        }
    };

    Ok(AcaTables::from_inputs_with_rules(base_year, fpl, applicable, rules))
}
