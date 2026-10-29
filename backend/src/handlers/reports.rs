//! Report generation and CSV/PDF export (roadmap Phase 5, features 6 & 7):
//! renders the data the projection engine and account rows already compute
//! into downloadable documents — a user can hand these to an accountant,
//! load them into a spreadsheet, or keep a printable copy of the plan. Every
//! render function here is a pure function of already-loaded data so it can
//! be unit tested without a database or HTTP stack; the handlers are thin
//! wrappers that load data and set the response headers.

use actix_web::{get, web, HttpResponse};
use diesel::prelude::*;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::projection::build_projection;
use crate::models::{Account, ProjectionResponse};
use crate::schema::accounts;

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

/// Export the annual plan report as CSV: one row per projection year with
/// net worth, withdrawals, cash flow, RMDs, Roth conversions, and taxes —
/// covers the roadmap's "Annual reports," "Withdrawal report," "Net worth,"
/// "Cash flow," "Future projections," and "Estate projection" report types
/// (the ending-balance trend across all years *is* the estate projection) in
/// one coherent table.
#[utoipa::path(
    get,
    path = "/api/reports/annual-plan.csv",
    tag = "reports",
    responses(
        (status = 200, description = "CSV annual plan report, one row per projection year", content_type = "text/csv"),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/reports/annual-plan.csv")]
pub async fn get_annual_plan_csv(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let projection = build_projection(&pool, auth.user_id.clone()).await?;
    let csv = render_annual_plan_csv(&projection);
    Ok(HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"annual-plan.csv\"",
        ))
        .body(csv))
}

/// Export the annual plan report as a printable PDF — the same figures as
/// [`get_annual_plan_csv`] in a shareable document a user might print or hand
/// to a spouse or advisor.
#[utoipa::path(
    get,
    path = "/api/reports/annual-plan.pdf",
    tag = "reports",
    responses(
        (status = 200, description = "PDF annual plan report", content_type = "application/pdf"),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "PDF rendering failed"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/reports/annual-plan.pdf")]
pub async fn get_annual_plan_pdf(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let projection = build_projection(&pool, auth.user_id.clone()).await?;
    let pdf = render_annual_plan_pdf(&projection)
        .map_err(|e| AppError::Internal(format!("failed to render PDF report: {e}")))?;
    Ok(HttpResponse::Ok()
        .content_type("application/pdf")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"annual-plan.pdf\"",
        ))
        .body(pdf))
}

/// Export the near-term quarterly withdrawal schedule as CSV (roadmap's
/// "Quarterly reports"): one row per quarter with income, spending,
/// estimated tax, and the per-account withdrawal breakdown.
#[utoipa::path(
    get,
    path = "/api/reports/quarterly-schedule.csv",
    tag = "reports",
    responses(
        (status = 200, description = "CSV quarterly withdrawal schedule", content_type = "text/csv"),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/reports/quarterly-schedule.csv")]
pub async fn get_quarterly_schedule_csv(
    pool: web::Data<DbPool>,
    auth: AuthUser,
) -> AppResult<HttpResponse> {
    let projection = build_projection(&pool, auth.user_id.clone()).await?;
    let csv = render_quarterly_schedule_csv(&projection);
    Ok(HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"quarterly-schedule.csv\"",
        ))
        .body(csv))
}

/// Export a snapshot of current account balances and allocations as CSV
/// (roadmap's "Asset allocation" report) — a point-in-time view of live
/// accounts, not a multi-year projection.
#[utoipa::path(
    get,
    path = "/api/reports/asset-allocation.csv",
    tag = "reports",
    responses(
        (status = 200, description = "CSV asset allocation snapshot, one row per account", content_type = "text/csv"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/reports/asset-allocation.csv")]
pub async fn get_asset_allocation_csv(
    pool: web::Data<DbPool>,
    auth: AuthUser,
) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let account_rows = web::block(move || -> AppResult<Vec<Account>> {
        let mut conn = pool.get()?;
        let rows = accounts::table
            .filter(accounts::user_id.eq(&user_id))
            .select(Account::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let csv = render_asset_allocation_csv(&account_rows);
    Ok(HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"asset-allocation.csv\"",
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
         Early Withdrawal Penalty,Federal Tax,State Taxable Income,State Tax,Total Tax,\
         Effective Rate,Marginal Rate,Roth Conversion,Medicare Premiums,IRMAA Surcharge\n",
    );
    for y in &projection.annual {
        let t = &y.tax;
        out.push_str(&format!(
            "{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.4},{:.4},{:.2},{:.2},{:.2}\n",
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
            t.early_withdrawal_penalty,
            t.federal_tax,
            t.state_taxable_income,
            t.state_tax,
            t.total_tax,
            t.effective_rate,
            t.marginal_rate,
            y.roth_conversion,
            y.medicare_premiums,
            y.irmaa_surcharge,
        ));
    }
    out
}

/// Render the annual-plan CSV body: one row per projection year covering net
/// worth, cash flow, withdrawals, RMDs, Roth conversions, and taxes.
fn render_annual_plan_csv(projection: &ProjectionResponse) -> String {
    let mut out = String::new();
    out.push_str(
        "Year,Age,Starting Balance,Income,Spending,Life Events Net,Growth,Withdrawals,\
         RMD Amount,Roth Conversion,Medicare Premiums,IRMAA Surcharge,Contributions,\
         Total Tax,Ending Balance,Shortfall\n",
    );
    for y in &projection.annual {
        out.push_str(&format!(
            "{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}\n",
            y.year,
            y.primary_age,
            y.starting_balance,
            y.income,
            y.spending,
            y.life_events_net,
            y.growth,
            y.withdrawals,
            y.rmd_amount,
            y.roth_conversion,
            y.medicare_premiums,
            y.irmaa_surcharge,
            y.contributions,
            y.taxes,
            y.ending_balance,
            y.shortfall,
        ));
    }
    out
}

/// A permissively-licensed font family (DejaVu Sans, vendored under
/// `backend/assets/fonts/`, see the `LICENSE` file there) embedded directly
/// in the binary so PDF rendering has no runtime dependency on an assets
/// directory being deployed alongside it. genpdf has no zero-file "standard
/// PDF font" mode — it always needs real glyph data to lay out and embed.
fn embedded_font_family() -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>, genpdf::error::Error> {
    let regular = include_bytes!("../../assets/fonts/DejaVuSans-Regular.ttf").to_vec();
    let bold = include_bytes!("../../assets/fonts/DejaVuSans-Bold.ttf").to_vec();
    let italic = include_bytes!("../../assets/fonts/DejaVuSans-Italic.ttf").to_vec();
    let bold_italic = include_bytes!("../../assets/fonts/DejaVuSans-BoldItalic.ttf").to_vec();
    Ok(genpdf::fonts::FontFamily {
        regular: genpdf::fonts::FontData::new(regular, None)?,
        bold: genpdf::fonts::FontData::new(bold, None)?,
        italic: genpdf::fonts::FontData::new(italic, None)?,
        bold_italic: genpdf::fonts::FontData::new(bold_italic, None)?,
    })
}

/// Render the annual plan report as a printable PDF: a title, a one-line plan
/// summary, and a table with the same per-year figures as
/// [`render_annual_plan_csv`], narrowed to the columns that fit a printed
/// page.
fn render_annual_plan_pdf(projection: &ProjectionResponse) -> Result<Vec<u8>, genpdf::error::Error> {
    use genpdf::elements::{FrameCellDecorator, Paragraph, TableLayout};
    use genpdf::style::Style;
    use genpdf::{Document, Element, SimplePageDecorator};

    let mut doc = Document::new(embedded_font_family()?);
    doc.set_title("Annual Plan Report");
    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(10);
    doc.set_page_decorator(decorator);

    doc.push(
        Paragraph::new("Annual Plan Report").styled(Style::new().bold().with_font_size(18)),
    );
    doc.push(Paragraph::new(format!(
        "Projected {}\u{2013}{} \u{2022} Lifetime taxes {:.0} \u{2022} Projected ending balance {:.0}",
        projection.start_year,
        projection.end_year,
        projection.summary.total_lifetime_taxes,
        projection.summary.projected_ending_balance,
    )));
    doc.push(genpdf::elements::Break::new(1));

    // Column weights are verified by rendering and visually inspecting the
    // output rather than purely computed. genpdf 0.2.0 has a rendering bug
    // where a bold, single-word cell in a narrow table column silently
    // renders as blank (confirmed empirically: identical text renders fine
    // once bold styling is removed) — so header cells here are deliberately
    // left unstyled rather than bold, and columns are widened for the
    // longest one-word label ("Withdrawals").
    let mut table = TableLayout::new(vec![2, 1, 3, 2, 3, 2, 2, 3]);
    table.set_cell_decorator(FrameCellDecorator::new(true, true, false));
    let header = [
        "Year",
        "Age",
        "Starting Bal.",
        "Income",
        "Withdrawals",
        "Total Tax",
        "Roth Conv.",
        "Ending Bal.",
    ];
    let mut header_row = table.row();
    for h in header {
        header_row.push_element(Paragraph::new(h).padded(1));
    }
    header_row.push()?;

    for y in &projection.annual {
        let cells = [
            y.year.to_string(),
            y.primary_age.to_string(),
            format!("{:.0}", y.starting_balance),
            format!("{:.0}", y.income),
            format!("{:.0}", y.withdrawals),
            format!("{:.0}", y.taxes),
            format!("{:.0}", y.roth_conversion),
            format!("{:.0}", y.ending_balance),
        ];
        let mut row = table.row();
        for cell in cells {
            row.push_element(Paragraph::new(cell).padded(1));
        }
        row.push()?;
    }
    doc.push(table);

    let mut buf = Vec::new();
    doc.render(&mut buf)?;
    Ok(buf)
}

/// Render the quarterly-schedule CSV body: one row per quarter of the
/// near-term withdrawal schedule, with the per-account withdrawal breakdown
/// folded into a single semicolon-separated column (consistent with the rest
/// of this module, string fields here are never quoted/escaped — account
/// names containing a comma would break the format, the same limitation
/// every export in this module already has).
fn render_quarterly_schedule_csv(projection: &ProjectionResponse) -> String {
    let mut out = String::new();
    out.push_str("Label,Year,Quarter,Income,Spending,Estimated Tax,Total Withdrawal,Withdrawal Breakdown\n");
    for q in &projection.quarterly {
        let breakdown = q
            .withdrawals
            .iter()
            .map(|w| format!("{}: {:.2}", w.account_name, w.amount))
            .collect::<Vec<_>>()
            .join("; ");
        out.push_str(&format!(
            "{},{},{},{:.2},{:.2},{:.2},{:.2},{}\n",
            q.label, q.year, q.quarter, q.income, q.spending, q.estimated_tax, q.total_withdrawal, breakdown,
        ));
    }
    out
}

/// Render the asset-allocation CSV body: one row per current account, a
/// point-in-time snapshot rather than a multi-year projection.
fn render_asset_allocation_csv(accounts: &[Account]) -> String {
    let mut out = String::new();
    out.push_str("Account,Category,Account Type,Owner,Current Balance,Stock %,Bond %,Cash %\n");
    let pct = |p: Option<i32>| p.map(|v| v.to_string()).unwrap_or_default();
    for a in accounts {
        out.push_str(&format!(
            "{},{},{},{},{:.2},{},{},{}\n",
            a.name,
            a.category,
            a.account_type,
            a.owner,
            a.current_balance,
            pct(a.allocation_stock_pct),
            pct(a.allocation_bond_pct),
            pct(a.allocation_cash_pct),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        EstimatedTaxes, ProjectionAssumptions, ProjectionSummary, YearAca, YearIrmaa, YearTax,
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
            irmaa_surcharge: 0.0,
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
                early_withdrawal_penalty: 0.0,
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
            irmaa: YearIrmaa::default(),
            ending_balance: 0.0,
            account_balances: vec![],
            shortfall: 0.0,
            withdrawal_sources: vec![],
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
                avoid_early_withdrawal_penalty: false,
                medicare_enrollment_age: 65,
                roth_conversion_mode: "fixed_ceiling".to_string(),
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
                total_lifetime_irmaa_surcharges: 0.0,
                total_lifetime_rmd: 0.0,
                total_lifetime_social_security: 0.0,
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

    // ---- Annual plan report --------------------------------------------

    #[test]
    fn annual_plan_csv_has_one_header_row_and_one_row_per_projection_year() {
        let csv = render_annual_plan_csv(&sample_projection());
        let lines: Vec<&str> = csv.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3); // header + 2 years
        assert!(lines[0].starts_with("Year,Age,Starting Balance,"));
        assert!(lines[1].starts_with("2026,65,"));
        assert!(lines[2].starts_with("2027,65,"));
    }

    #[test]
    fn annual_plan_pdf_renders_a_non_empty_pdf_document() {
        let pdf = render_annual_plan_pdf(&sample_projection()).expect("PDF should render");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-"), "output should be a valid PDF");
    }

    #[test]
    fn annual_plan_pdf_renders_with_no_projection_years() {
        let mut projection = sample_projection();
        projection.annual.clear();
        let pdf = render_annual_plan_pdf(&projection).expect("PDF should render with just a header");
        assert!(pdf.starts_with(b"%PDF-"));
    }

    // ---- Quarterly schedule report --------------------------------------

    fn sample_quarter(label: &str, year: i32, quarter: i32) -> crate::models::QuarterProjection {
        crate::models::QuarterProjection {
            label: label.to_string(),
            year,
            quarter,
            income: 5_000.0,
            spending: 8_000.0,
            estimated_tax: 1_200.0,
            total_withdrawal: 4_200.0,
            withdrawals: vec![crate::models::QuarterWithdrawal {
                account_id: "a1".into(),
                account_name: "Brokerage".into(),
                category: "taxable".into(),
                amount: 4_200.0,
            }],
        }
    }

    #[test]
    fn quarterly_schedule_csv_has_one_row_per_quarter_with_the_withdrawal_breakdown() {
        let mut projection = sample_projection();
        projection.quarterly = vec![
            sample_quarter("2026 Q1", 2026, 1),
            sample_quarter("2026 Q2", 2026, 2),
        ];
        let csv = render_quarterly_schedule_csv(&projection);
        let lines: Vec<&str> = csv.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3); // header + 2 quarters
        assert!(lines[1].starts_with("2026 Q1,2026,1,5000.00,8000.00,1200.00,4200.00,"));
        assert!(lines[1].contains("Brokerage: 4200.00"));
    }

    #[test]
    fn quarterly_schedule_csv_is_header_only_with_no_quarters() {
        let csv = render_quarterly_schedule_csv(&sample_projection());
        assert_eq!(csv.trim_end().split('\n').count(), 1);
    }

    // ---- Asset allocation report -----------------------------------------

    fn sample_account(name: &str, category: &str, balance: f64, stock_pct: Option<i32>) -> Account {
        let now: chrono::NaiveDateTime = "2026-01-01T00:00:00".parse().unwrap();
        Account {
            id: "a1".into(),
            user_id: "u1".into(),
            name: name.to_string(),
            category: category.to_string(),
            account_type: "brokerage".into(),
            owner: "self".into(),
            current_balance: balance,
            expected_roi: 6.0,
            dividend_yield: 0.0,
            cost_basis: None,
            allocation_stock_pct: stock_pct,
            allocation_bond_pct: stock_pct.map(|p| 100 - p),
            allocation_cash_pct: None,
            withdrawal_restrictions: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn asset_allocation_csv_has_one_row_per_account() {
        let accounts = vec![
            sample_account("Brokerage", "taxable", 100_000.0, Some(80)),
            sample_account("IRA", "tax_deferred", 250_000.0, None),
        ];
        let csv = render_asset_allocation_csv(&accounts);
        let lines: Vec<&str> = csv.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3); // header + 2 accounts
        assert!(lines[1].starts_with("Brokerage,taxable,brokerage,self,100000.00,80,20,"));
        // Missing allocation percentages render as empty fields, not "0".
        assert!(lines[2].starts_with("IRA,tax_deferred,brokerage,self,250000.00,,,"));
    }

    #[test]
    fn asset_allocation_csv_is_header_only_with_no_accounts() {
        let csv = render_asset_allocation_csv(&[]);
        assert_eq!(csv.trim_end().split('\n').count(), 1);
    }
}
