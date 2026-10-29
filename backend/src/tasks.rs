//! Task lists and reminders (roadmap Phase 5, feature 5): derives the
//! current quarter's and year's actionable to-dos ("Take a withdrawal,"
//! "Pay estimated taxes," "Convert to Roth," "RMD due," "Renew ACA
//! coverage," upcoming regulatory deadlines) from data the projection engine
//! already computes — mirrors how `crate::insights` derives alerts from the
//! same kind of data. Task *content* is never persisted, only which tasks a
//! user has checked off (`crate::models::TaskCompletion`), keyed by the
//! stable `key` each task carries here.

use serde::Serialize;
use utoipa::ToSchema;

use crate::models::{QuarterProjection, YearProjection};

/// A withdrawal line below this amount isn't worth a dedicated task.
const WITHDRAWAL_TASK_THRESHOLD: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    Withdrawal,
    EstimatedTax,
    RothConversion,
    Rmd,
    AcaRenewal,
    Regulatory,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Task {
    /// Stable, deterministic identifier (task kind + period, and account for
    /// withdrawals) so completion persists across regenerations even though
    /// the task list itself is never stored. URL-path-safe: spaces in
    /// period labels and milestone titles are replaced with hyphens.
    pub key: String,
    pub category: TaskCategory,
    pub title: String,
    pub detail: String,
    /// Human-friendly period the task applies to, e.g. "2026 Q1" or "2026".
    pub period: String,
    /// Whether the user has checked this task off, from `task_completions`.
    /// Always `false` from `derive_tasks` itself; the handler fills this in.
    pub completed: bool,
}

pub struct TaskContext<'a> {
    pub current_year: i32,
    pub current_year_projection: Option<&'a YearProjection>,
    pub current_quarter_projection: Option<&'a QuarterProjection>,
}

/// Replace spaces with hyphens so a label can be embedded in a URL path
/// segment (`POST /api/tasks/{key}/complete`) without percent-encoding.
fn slug(label: &str) -> String {
    label.replace(' ', "-")
}

pub fn derive_tasks(ctx: &TaskContext) -> Vec<Task> {
    let mut tasks = Vec::new();

    if let Some(qp) = ctx.current_quarter_projection {
        let period = slug(&qp.label);
        for w in &qp.withdrawals {
            if w.amount > WITHDRAWAL_TASK_THRESHOLD {
                tasks.push(Task {
                    key: format!("withdrawal:{period}:{}", w.account_id),
                    category: TaskCategory::Withdrawal,
                    title: format!("Take a ${:.0} withdrawal from {}", w.amount, w.account_name),
                    detail: format!("Planned for {}.", qp.label),
                    period: qp.label.clone(),
                    completed: false,
                });
            }
        }
        if qp.estimated_tax > 0.0 {
            tasks.push(Task {
                key: format!("estimated_tax:{period}"),
                category: TaskCategory::EstimatedTax,
                title: format!("Pay {} estimated taxes: ${:.0}", qp.label, qp.estimated_tax),
                detail: "Based on this quarter's projected tax liability.".into(),
                period: qp.label.clone(),
                completed: false,
            });
        }
    }

    if let Some(yp) = ctx.current_year_projection {
        let year = ctx.current_year;
        if yp.roth_conversion > 0.0 {
            tasks.push(Task {
                key: format!("roth_conversion:{year}"),
                category: TaskCategory::RothConversion,
                title: format!("Convert ${:.0} to Roth this year", yp.roth_conversion),
                detail: format!("Planned for {year}."),
                period: year.to_string(),
                completed: false,
            });
        }
        if yp.rmd_amount > 0.0 {
            tasks.push(Task {
                key: format!("rmd:{year}"),
                category: TaskCategory::Rmd,
                title: format!("Take your ${:.0} required minimum distribution", yp.rmd_amount),
                detail: format!("Due by December 31, {year}."),
                period: year.to_string(),
                completed: false,
            });
        }
        if yp.aca.eligible && yp.aca.benchmark_premium > 0.0 {
            tasks.push(Task {
                key: format!("aca_renewal:{year}"),
                category: TaskCategory::AcaRenewal,
                title: "Renew your ACA marketplace coverage".into(),
                detail: "Confirm your plan and income estimate for open enrollment.".into(),
                period: year.to_string(),
                completed: false,
            });
        }
        for m in yp.milestones.iter().filter(|m| m.is_regulatory) {
            tasks.push(Task {
                key: format!("milestone:{year}:{}", slug(&m.label)),
                category: TaskCategory::Regulatory,
                title: m.label.clone(),
                detail: m.detail.clone(),
                period: year.to_string(),
                completed: false,
            });
        }
    }

    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Milestone, QuarterWithdrawal, YearAca, YearIrmaa, YearTax,
    };

    fn sample_quarter() -> QuarterProjection {
        QuarterProjection {
            label: "2026 Q1".into(),
            year: 2026,
            quarter: 1,
            income: 5_000.0,
            spending: 8_000.0,
            estimated_tax: 1_200.0,
            total_withdrawal: 4_200.0,
            withdrawals: vec![
                QuarterWithdrawal {
                    account_id: "a1".into(),
                    account_name: "Brokerage".into(),
                    category: "taxable".into(),
                    amount: 4_200.0,
                },
                QuarterWithdrawal {
                    account_id: "a2".into(),
                    account_name: "Petty cash".into(),
                    category: "taxable".into(),
                    amount: 10.0, // below threshold
                },
            ],
        }
    }

    fn sample_year(
        roth_conversion: f64,
        rmd_amount: f64,
        aca: YearAca,
        milestones: Vec<Milestone>,
    ) -> YearProjection {
        YearProjection {
            year: 2026,
            primary_age: 68,
            spouse_age: None,
            starting_balance: 0.0,
            income: 0.0,
            spending: 0.0,
            life_events_net: 0.0,
            life_events: vec![],
            milestones,
            growth: 0.0,
            withdrawals: 0.0,
            rmd_amount,
            medicare_premiums: 0.0,
            irmaa_surcharge: 0.0,
            contributions: 0.0,
            roth_conversion,
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
                early_withdrawal_penalty: 0.0,
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
            irmaa: YearIrmaa::default(),
            ending_balance: 0.0,
            account_balances: vec![],
            shortfall: 0.0,
            withdrawal_sources: vec![],
        }
    }

    #[test]
    fn withdrawal_tasks_skip_amounts_at_or_below_the_threshold() {
        let quarter = sample_quarter();
        let ctx = TaskContext {
            current_year: 2026,
            current_year_projection: None,
            current_quarter_projection: Some(&quarter),
        };
        let tasks = derive_tasks(&ctx);
        let withdrawal_titles: Vec<&str> = tasks
            .iter()
            .filter(|t| t.category == TaskCategory::Withdrawal)
            .map(|t| t.title.as_str())
            .collect();
        assert_eq!(withdrawal_titles.len(), 1);
        assert!(withdrawal_titles[0].contains("Brokerage"));
    }

    #[test]
    fn estimated_tax_task_uses_a_url_safe_key() {
        let quarter = sample_quarter();
        let ctx = TaskContext {
            current_year: 2026,
            current_year_projection: None,
            current_quarter_projection: Some(&quarter),
        };
        let tasks = derive_tasks(&ctx);
        let tax_task = tasks
            .iter()
            .find(|t| t.category == TaskCategory::EstimatedTax)
            .expect("expected an estimated tax task");
        assert_eq!(tax_task.key, "estimated_tax:2026-Q1");
        assert!(!tax_task.key.contains(' '));
    }

    #[test]
    fn roth_conversion_and_rmd_tasks_only_appear_when_positive() {
        let year_with_both = sample_year(50_000.0, 12_000.0, YearAca::default(), vec![]);
        let ctx = TaskContext {
            current_year: 2026,
            current_year_projection: Some(&year_with_both),
            current_quarter_projection: None,
        };
        let tasks = derive_tasks(&ctx);
        assert!(tasks.iter().any(|t| t.category == TaskCategory::RothConversion));
        assert!(tasks.iter().any(|t| t.category == TaskCategory::Rmd));

        let year_with_neither = sample_year(0.0, 0.0, YearAca::default(), vec![]);
        let ctx_none = TaskContext {
            current_year: 2026,
            current_year_projection: Some(&year_with_neither),
            current_quarter_projection: None,
        };
        let tasks_none = derive_tasks(&ctx_none);
        assert!(!tasks_none.iter().any(|t| t.category == TaskCategory::RothConversion));
        assert!(!tasks_none.iter().any(|t| t.category == TaskCategory::Rmd));
    }

    #[test]
    fn aca_renewal_task_only_appears_when_eligible_with_a_benchmark_premium() {
        let mut aca = YearAca::default();
        aca.eligible = true;
        aca.benchmark_premium = 12_000.0;
        let year = sample_year(0.0, 0.0, aca, vec![]);
        let ctx = TaskContext {
            current_year: 2026,
            current_year_projection: Some(&year),
            current_quarter_projection: None,
        };
        assert!(derive_tasks(&ctx).iter().any(|t| t.category == TaskCategory::AcaRenewal));
    }

    #[test]
    fn only_regulatory_milestones_become_tasks() {
        let milestones = vec![
            Milestone {
                label: "Medicare enrollment window opens".into(),
                detail: "detail".into(),
                age: 64,
                is_regulatory: true,
            },
            Milestone {
                label: "Social Security eligibility".into(),
                detail: "detail".into(),
                age: 62,
                is_regulatory: false,
            },
        ];
        let year = sample_year(0.0, 0.0, YearAca::default(), milestones);
        let ctx = TaskContext {
            current_year: 2026,
            current_year_projection: Some(&year),
            current_quarter_projection: None,
        };
        let tasks = derive_tasks(&ctx);
        let regulatory: Vec<&Task> = tasks
            .iter()
            .filter(|t| t.category == TaskCategory::Regulatory)
            .collect();
        assert_eq!(regulatory.len(), 1);
        assert_eq!(regulatory[0].key, "milestone:2026:Medicare-enrollment-window-opens");
    }

    #[test]
    fn no_tasks_when_nothing_is_due() {
        let ctx = TaskContext {
            current_year: 2026,
            current_year_projection: None,
            current_quarter_projection: None,
        };
        assert!(derive_tasks(&ctx).is_empty());
    }
}
