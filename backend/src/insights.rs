//! Personalized insights & anomaly detection (roadmap Phase 6, feature 6),
//! mapped to the roadmap's "Alerts" list. Pure derived data — no new tables —
//! built from figures the projection engine, quarterly review, and Monte
//! Carlo simulation already compute, so this module only re-reads and
//! thresholds them. Kept free of I/O so it's unit-testable without a
//! database.

use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use utoipa::ToSchema;

use crate::models::{Account, YearProjection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InsightSeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InsightCategory {
    AcaSubsidy,
    Irmaa,
    Rmd,
    NegativeCashFlow,
    UnexpectedSpending,
    QuarterlyReviewDue,
    AggressivePortfolio,
    SequenceOfReturnRisk,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Insight {
    pub category: InsightCategory,
    pub severity: InsightSeverity,
    pub title: String,
    pub message: String,
}

/// The most recently completed quarterly review's planned-vs-actual figures,
/// as already computed by `QuarterlyReviewResponse`/`ReconciliationSummary`.
pub struct ReviewVariance {
    pub label: String,
    pub planned_spending: f64,
    pub spending_variance: f64,
    pub net_cash_flow: f64,
}

pub struct InsightContext<'a> {
    pub current_year: i32,
    pub primary_age: i32,
    pub retirement_date: NaiveDate,
    /// This year's row from the projection engine's `annual` output, if a
    /// projection could be built (requires a saved profile).
    pub current_year_projection: Option<&'a YearProjection>,
    pub accounts: &'a [Account],
    /// True when the current quarter has no completed review yet.
    pub review_due: bool,
    pub latest_review: Option<ReviewVariance>,
    /// Probability of plan success from a light-weight Monte Carlo run.
    pub monte_carlo_success_rate: Option<f64>,
}

/// A deviation of more than this fraction between actual and planned
/// spending is flagged as "unexpected."
const SPENDING_VARIANCE_THRESHOLD: f64 = 0.15;
/// MAGI at or above this percent of the federal poverty line is flagged as
/// "approaching" the point where the ACA subsidy phases out to zero.
const ACA_APPROACHING_PHASEOUT_FPL_PERCENT: f64 = 350.0;
/// Simple age-based glide-path heuristic: a stock allocation more than this
/// many points above `110 - age` is flagged as potentially too aggressive.
const AGGRESSIVE_ALLOCATION_MARGIN_PCT: i32 = 15;
/// Monte Carlo success rate below this is flagged as elevated risk.
const SEQUENCE_RISK_SUCCESS_THRESHOLD: f64 = 0.70;
/// Sequence-of-return risk is only flagged within this many years of
/// retirement, when an early downturn does the most lasting damage.
const SEQUENCE_RISK_WINDOW_YEARS: i32 = 5;

pub fn generate_insights(ctx: &InsightContext) -> Vec<Insight> {
    let mut insights = Vec::new();

    if let Some(yp) = ctx.current_year_projection {
        aca_insight(yp, &mut insights);
        irmaa_insight(yp, &mut insights);
        rmd_insight(yp, &mut insights);
    }

    if let Some(review) = &ctx.latest_review {
        if review.net_cash_flow < 0.0 {
            insights.push(Insight {
                category: InsightCategory::NegativeCashFlow,
                severity: InsightSeverity::High,
                title: "Negative cash flow last quarter".to_string(),
                message: format!(
                    "{} had a net cash flow of {:.2} — spending and taxes exceeded income and withdrawals.",
                    review.label, review.net_cash_flow
                ),
            });
        }
        if review.planned_spending > 0.0 {
            let deviation = (review.spending_variance / review.planned_spending).abs();
            if deviation > SPENDING_VARIANCE_THRESHOLD {
                insights.push(Insight {
                    category: InsightCategory::UnexpectedSpending,
                    severity: InsightSeverity::Medium,
                    title: "Spending deviated from plan".to_string(),
                    message: format!(
                        "{} actual spending differed from the plan by {:.0}% ({:+.2}).",
                        review.label,
                        deviation * 100.0,
                        review.spending_variance
                    ),
                });
            }
        }
    }

    if ctx.review_due {
        insights.push(Insight {
            category: InsightCategory::QuarterlyReviewDue,
            severity: InsightSeverity::Low,
            title: "Quarterly review due".to_string(),
            message: "This quarter hasn't been reviewed yet — enter actuals to keep the plan current.".to_string(),
        });
    }

    if let Some(insight) = aggressive_portfolio_insight(ctx.primary_age, ctx.accounts) {
        insights.push(insight);
    }

    if let Some(rate) = ctx.monte_carlo_success_rate {
        let years_into_retirement = ctx.current_year - ctx.retirement_date.year();
        if (0..=SEQUENCE_RISK_WINDOW_YEARS).contains(&years_into_retirement)
            && rate < SEQUENCE_RISK_SUCCESS_THRESHOLD
        {
            insights.push(Insight {
                category: InsightCategory::SequenceOfReturnRisk,
                severity: InsightSeverity::High,
                title: "Elevated sequence-of-return risk".to_string(),
                message: format!(
                    "Plan success probability is {:.0}% within the first {SEQUENCE_RISK_WINDOW_YEARS} years of retirement, when early downturns do the most lasting damage.",
                    rate * 100.0
                ),
            });
        }
    }

    insights.sort_by(|a, b| b.severity.cmp(&a.severity));
    insights
}

fn aca_insight(yp: &YearProjection, insights: &mut Vec<Insight>) {
    if yp.aca.benchmark_premium <= 0.0 {
        return;
    }
    if !yp.aca.eligible {
        insights.push(Insight {
            category: InsightCategory::AcaSubsidy,
            severity: InsightSeverity::High,
            title: "ACA subsidy income exceeded".to_string(),
            message: format!(
                "MAGI of {:.0} makes {} ineligible for an ACA premium tax credit this year.",
                yp.aca.magi, yp.year
            ),
        });
    } else if yp.aca.fpl_percent >= ACA_APPROACHING_PHASEOUT_FPL_PERCENT {
        insights.push(Insight {
            category: InsightCategory::AcaSubsidy,
            severity: InsightSeverity::Medium,
            title: "Approaching ACA subsidy phase-out".to_string(),
            message: format!(
                "MAGI is at {:.0}% of the federal poverty line in {} — the premium tax credit shrinks as income rises further.",
                yp.aca.fpl_percent, yp.year
            ),
        });
    }
}

fn irmaa_insight(yp: &YearProjection, insights: &mut Vec<Insight>) {
    if yp.irmaa.applies {
        insights.push(Insight {
            category: InsightCategory::Irmaa,
            severity: InsightSeverity::Medium,
            title: "IRMAA surcharge applies".to_string(),
            message: format!(
                "Medicare IRMAA surcharges of {:.2}/year apply in {}, based on MAGI from {}.",
                yp.irmaa.total_surcharge, yp.year, yp.irmaa.lookback_year
            ),
        });
    }
}

fn rmd_insight(yp: &YearProjection, insights: &mut Vec<Insight>) {
    if yp.rmd_amount > 0.0 {
        insights.push(Insight {
            category: InsightCategory::Rmd,
            severity: InsightSeverity::Medium,
            title: "RMD due this year".to_string(),
            message: format!(
                "A required minimum distribution of {:.2} is due by December 31, {}.",
                yp.rmd_amount, yp.year
            ),
        });
    }
}

fn aggressive_portfolio_insight(primary_age: i32, accounts: &[Account]) -> Option<Insight> {
    let (weighted_stock, total_balance) = accounts
        .iter()
        .filter_map(|a| a.allocation_stock_pct.map(|pct| (pct, a.current_balance)))
        .fold((0.0, 0.0), |(sum, bal), (pct, balance)| {
            (sum + pct as f64 * balance, bal + balance)
        });
    if total_balance <= 0.0 {
        return None;
    }
    let avg_stock_pct = weighted_stock / total_balance;
    let recommended_max = (110 - primary_age).max(0) as f64;
    if avg_stock_pct > recommended_max + AGGRESSIVE_ALLOCATION_MARGIN_PCT as f64 {
        Some(Insight {
            category: InsightCategory::AggressivePortfolio,
            severity: InsightSeverity::Medium,
            title: "Portfolio may be too aggressive".to_string(),
            message: format!(
                "Average stock allocation is {avg_stock_pct:.0}%, well above the ~{recommended_max:.0}% commonly suggested at age {primary_age}."
            ),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{YearAca, YearIrmaa, YearTax};
    use chrono::NaiveDateTime;

    fn sample_account(stock_pct: Option<i32>, balance: f64) -> Account {
        let now: NaiveDateTime = "2026-01-01T00:00:00".parse().unwrap();
        Account {
            id: "a1".into(),
            user_id: "u1".into(),
            name: "Brokerage".into(),
            category: "taxable".into(),
            account_type: "brokerage".into(),
            owner: "self".into(),
            current_balance: balance,
            expected_roi: 6.0,
            dividend_yield: 1.5,
            cost_basis: None,
            allocation_stock_pct: stock_pct,
            allocation_bond_pct: stock_pct.map(|p| 100 - p),
            allocation_cash_pct: stock_pct.map(|_| 0),
            withdrawal_restrictions: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_year(rmd: f64, aca: YearAca, irmaa: YearIrmaa) -> YearProjection {
        YearProjection {
            year: 2026,
            primary_age: 68,
            spouse_age: None,
            starting_balance: 0.0,
            income: 0.0,
            spending: 0.0,
            life_events_net: 0.0,
            life_events: vec![],
            milestones: vec![],
            growth: 0.0,
            withdrawals: 0.0,
            rmd_amount: rmd,
            medicare_premiums: 0.0,
            irmaa_surcharge: irmaa.total_surcharge,
            contributions: 0.0,
            roth_conversion: 0.0,
            taxes: 0.0,
            tax: YearTax {
                ordinary_income: 0.0,
                qualified_dividends: 0.0,
                capital_gains: 0.0,
                social_security_benefits: 0.0,
                taxable_social_security: 0.0,
                adjusted_gross_income: 0.0,
                magi: aca.magi,
                standard_deduction: 0.0,
                taxable_income: 0.0,
                federal_ordinary_tax: 0.0,
                federal_capital_gains_tax: 0.0,
                federal_tax: 0.0,
                state_taxable_income: 0.0,
                state_standard_deduction: 0.0,
                state_tax: 0.0,
                state_marginal_rate: 0.0,
                property_tax: 0.0,
                total_tax: 0.0,
                effective_rate: 0.0,
                marginal_rate: 0.0,
            },
            withdrawal_order: "taxable_first".into(),
            aca,
            irmaa,
            ending_balance: 0.0,
            account_balances: vec![],
            shortfall: 0.0,
        }
    }

    #[test]
    fn flags_rmd_due_when_amount_positive() {
        let yp = sample_year(12_000.0, YearAca::default(), YearIrmaa::default());
        let ctx = InsightContext {
            current_year: 2026,
            primary_age: 75,
            retirement_date: "2020-01-01".parse().unwrap(),
            current_year_projection: Some(&yp),
            accounts: &[],
            review_due: false,
            latest_review: None,
            monte_carlo_success_rate: None,
        };
        let insights = generate_insights(&ctx);
        assert!(insights.iter().any(|i| i.category == InsightCategory::Rmd));
    }

    #[test]
    fn flags_negative_cash_flow_as_high_severity() {
        let ctx = InsightContext {
            current_year: 2026,
            primary_age: 65,
            retirement_date: "2020-01-01".parse().unwrap(),
            current_year_projection: None,
            accounts: &[],
            review_due: false,
            latest_review: Some(ReviewVariance {
                label: "2026 Q1".into(),
                planned_spending: 10_000.0,
                spending_variance: 0.0,
                net_cash_flow: -500.0,
            }),
            monte_carlo_success_rate: None,
        };
        let insights = generate_insights(&ctx);
        let found = insights
            .iter()
            .find(|i| i.category == InsightCategory::NegativeCashFlow)
            .expect("expected a negative cash flow insight");
        assert_eq!(found.severity, InsightSeverity::High);
    }

    #[test]
    fn flags_unexpected_spending_over_threshold_but_not_under() {
        let ctx_over = InsightContext {
            current_year: 2026,
            primary_age: 65,
            retirement_date: "2020-01-01".parse().unwrap(),
            current_year_projection: None,
            accounts: &[],
            review_due: false,
            latest_review: Some(ReviewVariance {
                label: "2026 Q1".into(),
                planned_spending: 10_000.0,
                spending_variance: 2_000.0,
                net_cash_flow: 100.0,
            }),
            monte_carlo_success_rate: None,
        };
        assert!(generate_insights(&ctx_over)
            .iter()
            .any(|i| i.category == InsightCategory::UnexpectedSpending));

        let ctx_under = InsightContext {
            latest_review: Some(ReviewVariance {
                label: "2026 Q1".into(),
                planned_spending: 10_000.0,
                spending_variance: 500.0,
                net_cash_flow: 100.0,
            }),
            ..ctx_over
        };
        assert!(!generate_insights(&ctx_under)
            .iter()
            .any(|i| i.category == InsightCategory::UnexpectedSpending));
    }

    #[test]
    fn flags_aggressive_allocation_relative_to_age() {
        let accounts = vec![sample_account(Some(95), 100_000.0)];
        let ctx = InsightContext {
            current_year: 2026,
            primary_age: 70, // recommended max ~40%
            retirement_date: "2020-01-01".parse().unwrap(),
            current_year_projection: None,
            accounts: &accounts,
            review_due: false,
            latest_review: None,
            monte_carlo_success_rate: None,
        };
        assert!(generate_insights(&ctx)
            .iter()
            .any(|i| i.category == InsightCategory::AggressivePortfolio));
    }

    #[test]
    fn conservative_allocation_is_not_flagged() {
        let accounts = vec![sample_account(Some(40), 100_000.0)];
        let ctx = InsightContext {
            current_year: 2026,
            primary_age: 70,
            retirement_date: "2020-01-01".parse().unwrap(),
            current_year_projection: None,
            accounts: &accounts,
            review_due: false,
            latest_review: None,
            monte_carlo_success_rate: None,
        };
        assert!(!generate_insights(&ctx)
            .iter()
            .any(|i| i.category == InsightCategory::AggressivePortfolio));
    }

    #[test]
    fn sequence_of_return_risk_only_flagged_early_in_retirement() {
        let base = InsightContext {
            current_year: 2026,
            primary_age: 65,
            retirement_date: "2025-01-01".parse().unwrap(),
            current_year_projection: None,
            accounts: &[],
            review_due: false,
            latest_review: None,
            monte_carlo_success_rate: Some(0.5),
        };
        assert!(generate_insights(&base)
            .iter()
            .any(|i| i.category == InsightCategory::SequenceOfReturnRisk));

        let far_out = InsightContext {
            retirement_date: "2000-01-01".parse().unwrap(),
            ..base
        };
        assert!(!generate_insights(&far_out)
            .iter()
            .any(|i| i.category == InsightCategory::SequenceOfReturnRisk));
    }

    #[test]
    fn insights_are_sorted_highest_severity_first() {
        let ctx = InsightContext {
            current_year: 2026,
            primary_age: 65,
            retirement_date: "2025-01-01".parse().unwrap(),
            current_year_projection: None,
            accounts: &[],
            review_due: true, // Low
            latest_review: Some(ReviewVariance {
                label: "2026 Q1".into(),
                planned_spending: 10_000.0,
                spending_variance: 0.0,
                net_cash_flow: -1.0, // High
            }),
            monte_carlo_success_rate: None,
        };
        let insights = generate_insights(&ctx);
        assert_eq!(insights[0].severity, InsightSeverity::High);
        assert_eq!(insights[insights.len() - 1].severity, InsightSeverity::Low);
    }
}
