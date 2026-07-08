//! Tax reporting (roadmap Phase 2, feature 8): exports the full multi-year tax
//! breakdown the projection engine already computes as a downloadable CSV, so
//! a user can hand it to an accountant or load it into a spreadsheet. The
//! on-screen equivalent is rendered by the frontend directly from the existing
//! `GET /api/projection` response; this endpoint exists purely to produce a
//! portable document.

use actix_web::{get, web, HttpResponse};

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::handlers::projection::build_projection;
use crate::models::ProjectionResponse;

/// Export the multi-year tax summary as CSV: one row per projection year with
/// the full federal/state tax breakdown.
#[utoipa::path(
    get,
    path = "/api/reports/tax-summary.csv",
    tag = "reports",
    responses(
        (status = 200, description = "CSV tax summary, one row per projection year", content_type = "text/csv"),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/reports/tax-summary.csv")]
pub async fn get_tax_summary_csv(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let projection = build_projection(&pool, auth.user_id.clone()).await?;
    let csv = render_tax_summary_csv(&projection);
    Ok(HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"tax-summary.csv\"",
        ))
        .body(csv))
}

/// Render the tax-summary CSV body. A pure function of the projection
/// response so it can be unit tested without a database or HTTP stack.
fn render_tax_summary_csv(projection: &ProjectionResponse) -> String {
    let mut out = String::new();
    out.push_str(
        "Year,Age,Withdrawal Order,Ordinary Income,Qualified Dividends,Capital Gains,\
         Social Security Benefits,Taxable Social Security,Adjusted Gross Income,MAGI,\
         Standard Deduction,Taxable Income,Federal Ordinary Tax,Federal Capital Gains Tax,\
         Federal Tax,State Taxable Income,State Tax,Total Tax,Effective Rate,Marginal Rate,\
         Roth Conversion,Medicare Premiums\n",
    );
    for y in &projection.annual {
        let t = &y.tax;
        out.push_str(&format!(
            "{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.4},{:.4},{:.2},{:.2}\n",
            y.year,
            y.primary_age,
            y.withdrawal_order,
            t.ordinary_income,
            t.qualified_dividends,
            t.capital_gains,
            t.social_security_benefits,
            t.taxable_social_security,
            t.adjusted_gross_income,
            t.magi,
            t.standard_deduction,
            t.taxable_income,
            t.federal_ordinary_tax,
            t.federal_capital_gains_tax,
            t.federal_tax,
            t.state_taxable_income,
            t.state_tax,
            t.total_tax,
            t.effective_rate,
            t.marginal_rate,
            y.roth_conversion,
            y.medicare_premiums,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        EstimatedTaxes, ProjectionAssumptions, ProjectionSummary, YearAca, YearTax,
    };

    fn sample_year(year: i32, ordinary_income: f64, total_tax: f64) -> crate::models::YearProjection {
        crate::models::YearProjection {
            year,
            primary_age: 65,
            spouse_age: None,
            starting_balance: 0.0,
            income: 0.0,
            spending: 0.0,
            life_events_net: 0.0,
            life_events: vec![],
            milestones: vec![],
            growth: 0.0,
            withdrawals: 0.0,
            rmd_amount: 0.0,
            medicare_premiums: 0.0,
            contributions: 0.0,
            roth_conversion: 0.0,
            taxes: total_tax,
            tax: YearTax {
                ordinary_income,
                qualified_dividends: 0.0,
                capital_gains: 0.0,
                social_security_benefits: 0.0,
                taxable_social_security: 0.0,
                adjusted_gross_income: ordinary_income,
                magi: ordinary_income,
                standard_deduction: 15_000.0,
                taxable_income: (ordinary_income - 15_000.0).max(0.0),
                federal_ordinary_tax: total_tax,
                federal_capital_gains_tax: 0.0,
                federal_tax: total_tax,
                state_taxable_income: 0.0,
                state_standard_deduction: 0.0,
                state_tax: 0.0,
                state_marginal_rate: 0.0,
                property_tax: 0.0,
                total_tax,
                effective_rate: if ordinary_income > 0.0 {
                    total_tax / ordinary_income
                } else {
                    0.0
                },
                marginal_rate: 0.12,
            },
            withdrawal_order: "taxable_first".to_string(),
            aca: YearAca::default(),
            ending_balance: 0.0,
            shortfall: 0.0,
        }
    }

    fn sample_projection() -> ProjectionResponse {
        ProjectionResponse {
            current_year: 2026,
            start_year: 2026,
            end_year: 2027,
            assumptions: ProjectionAssumptions {
                inflation_rate: 0.0,
                investment_return_rate: 0.0,
                healthcare_inflation_rate: 0.0,
                social_security_cola_rate: 0.0,
                roth_conversion_ceiling: 0.0,
                roth_conversion_start_year: None,
                roth_conversion_end_year: None,
                withdrawal_strategy: "conventional".to_string(),
                aca_benchmark_annual_premium: 0.0,
                medicare_part_b_annual_premium: 0.0,
                is_default: false,
            },
            summary: ProjectionSummary {
                current_net_worth: 0.0,
                projected_ending_balance: 0.0,
                total_lifetime_income: 0.0,
                total_lifetime_spending: 0.0,
                total_lifetime_withdrawals: 0.0,
                total_lifetime_taxes: 0.0,
                total_lifetime_federal_taxes: 0.0,
                total_lifetime_state_taxes: 0.0,
                total_lifetime_roth_conversions: 0.0,
                total_lifetime_aca_subsidies: 0.0,
                total_lifetime_medicare_premiums: 0.0,
                depletion_year: None,
            },
            annual: vec![
                sample_year(2026, 60_000.0, 5_914.0),
                sample_year(2027, 65_000.0, 6_500.0),
            ],
            quarterly: vec![],
            estimated_taxes: EstimatedTaxes {
                tax_year: 2026,
                total: 5_914.0,
                note: String::new(),
                payments: vec![],
            },
        }
    }

    #[test]
    fn csv_has_one_header_row_and_one_row_per_projection_year() {
        let csv = render_tax_summary_csv(&sample_projection());
        let lines: Vec<&str> = csv.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3); // header + 2 years
        assert!(lines[0].starts_with("Year,Age,Withdrawal Order,"));
    }

    #[test]
    fn csv_rows_carry_the_year_and_tax_figures() {
        let csv = render_tax_summary_csv(&sample_projection());
        let lines: Vec<&str> = csv.trim_end().split('\n').collect();
        assert!(lines[1].starts_with("2026,65,taxable_first,60000.00"));
        assert!(lines[2].starts_with("2027,65,taxable_first,65000.00"));
        assert!(lines[1].contains("5914.00"));
    }

    #[test]
    fn csv_body_is_empty_when_there_are_no_projection_years() {
        let mut projection = sample_projection();
        projection.annual.clear();
        let csv = render_tax_summary_csv(&projection);
        assert_eq!(csv.trim_end().split('\n').count(), 1); // header only
    }
}
