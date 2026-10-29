//! AI-powered explanations of recommendations (roadmap Phase 6, feature 5).
//!
//! Turns an `Insight` into a short, plain-English explanation of why it
//! matters and what the user might do about it. The provider is
//! config-driven (`AI_EXPLANATION_PROVIDER`):
//!
//! - `"claude"` calls the Anthropic Messages API (requires `ANTHROPIC_API_KEY`).
//! - anything else, including a missing/invalid Claude configuration, falls
//!   back to a deterministic template — no network dependency, no API key.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::insights::{Insight, InsightCategory, InsightSeverity};

/// Request body for `POST /insights/explain`: the frontend already holds an
/// `Insight` from `GET /insights` and sends it back verbatim rather than the
/// server recomputing (and possibly re-deriving a different) insight list.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ExplainInsightRequest {
    pub category: InsightCategory,
    pub severity: InsightSeverity,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExplainInsightResponse {
    pub explanation: String,
    pub provider: String,
}

/// The subset of plan context worth handing to an explanation generator,
/// alongside the `Insight` itself. Owned rather than borrowed so it can
/// cross the `.await` point in the Claude provider.
#[derive(Debug, Clone)]
pub struct ExplanationContext {
    pub primary_age: i32,
    pub current_year: i32,
    pub retirement_year: i32,
    pub monte_carlo_success_rate: Option<f64>,
}

pub struct ExplanationResult {
    pub explanation: String,
    pub provider: &'static str,
}

pub async fn explain_insight(
    config: &Config,
    insight: &Insight,
    context: &ExplanationContext,
) -> AppResult<ExplanationResult> {
    if config.ai_explanation_provider == "claude" {
        if let Some(api_key) = &config.anthropic_api_key {
            let explanation =
                claude_explanation(api_key, &config.anthropic_model, insight, context).await?;
            return Ok(ExplanationResult {
                explanation,
                provider: "claude",
            });
        }
        log::warn!(
            "AI_EXPLANATION_PROVIDER=claude but ANTHROPIC_API_KEY is unset; falling back to the template provider"
        );
    }
    Ok(ExplanationResult {
        explanation: template_explanation(insight, context),
        provider: "template",
    })
}

fn template_explanation(insight: &Insight, context: &ExplanationContext) -> String {
    let why = match insight.category {
        InsightCategory::AcaSubsidy => {
            "ACA premium tax credits phase out as modified adjusted gross income (MAGI) rises, so \
             which accounts you withdraw from — and how much taxable income that generates — \
             directly affects your health insurance costs before Medicare."
        }
        InsightCategory::Irmaa => {
            "Medicare Part B and Part D premiums include an income-related surcharge (IRMAA) based \
             on your MAGI from two years earlier. Roth conversions, capital gains, and withdrawal \
             timing all feed into that number."
        }
        InsightCategory::Rmd => {
            "Once you reach the required beginning date, the IRS mandates minimum withdrawals from \
             tax-deferred accounts whether or not you need the cash — missing one carries a steep \
             excise tax penalty."
        }
        InsightCategory::NegativeCashFlow => {
            "Spending and taxes exceeded income and withdrawals for the period, which draws down \
             account balances faster than planned and can compound if it isn't corrected."
        }
        InsightCategory::UnexpectedSpending => {
            "Spending that diverges meaningfully from plan changes how long your assets are \
             projected to last, and is worth reconciling into the assumptions driving future \
             quarters."
        }
        InsightCategory::QuarterlyReviewDue => {
            "Recording actual balances, spending, and withdrawals each quarter lets the planner \
             recalculate the rest of your plan from where you actually are, rather than \
             compounding forecast error."
        }
        InsightCategory::AggressivePortfolio => {
            "A stock allocation well above common age-based glide paths increases \
             sequence-of-return risk — the chance that a downturn early in retirement forces you \
             to sell depressed assets to fund withdrawals."
        }
        InsightCategory::SequenceOfReturnRisk => {
            "Poor investment returns in the first several years of retirement do outsized damage \
             because withdrawals lock in losses a portfolio has less time to recover from."
        }
        InsightCategory::RegulatoryMilestone => {
            "This milestone carries a real compliance deadline — an enrollment window, RMDs \
             starting, and so on — and missing it can mean penalties or a coverage gap."
        }
    };
    let success_rate = context
        .monte_carlo_success_rate
        .map(|r| format!(", {:.0}% plan success probability", r * 100.0))
        .unwrap_or_default();
    format!(
        "{}\n\n{}\n\n(Based on your current plan at age {}, {}{}.)",
        insight.message, why, context.primary_age, context.current_year, success_rate
    )
}

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    thinking: ThinkingConfig,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

async fn claude_explanation(
    api_key: &str,
    model: &str,
    insight: &Insight,
    context: &ExplanationContext,
) -> AppResult<String> {
    let system = "You are a retirement planning assistant. In 2-4 plain-English sentences aimed at \
        a non-financial-professional, explain why the following alert or recommendation matters and \
        what general options the user has. Do not invent numbers beyond what's given. No preamble, \
        no headers, no markdown.";

    let success_rate = context
        .monte_carlo_success_rate
        .map(|r| format!(", Monte Carlo plan success probability {:.0}%", r * 100.0))
        .unwrap_or_default();
    let user_content = format!(
        "Insight category: {:?}\nTitle: {}\nDetail: {}\n\nUser context: age {}, plan year {}, \
         retirement year {}{}.",
        insight.category,
        insight.title,
        insight.message,
        context.primary_age,
        context.current_year,
        context.retirement_year,
        success_rate
    );

    let body = AnthropicRequest {
        model,
        max_tokens: 400,
        system,
        // Explanations are short and don't need extended reasoning; disabling
        // thinking keeps `max_tokens` entirely for the visible response.
        thinking: ThinkingConfig { kind: "disabled" },
        messages: vec![AnthropicMessage {
            role: "user",
            content: &user_content,
        }],
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Anthropic request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "Anthropic API returned {status}: {text}"
        )));
    }

    let parsed: AnthropicResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Anthropic response was unparseable: {e}")))?;

    parsed
        .content
        .into_iter()
        .find(|b| b.block_type == "text")
        .and_then(|b| b.text)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::Internal("Anthropic response had no text content".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::InsightSeverity;

    fn sample_insight() -> Insight {
        Insight {
            category: InsightCategory::Rmd,
            severity: InsightSeverity::Medium,
            title: "RMD due this year".to_string(),
            message: "A required minimum distribution of 12000.00 is due by December 31, 2026."
                .to_string(),
        }
    }

    fn sample_context() -> ExplanationContext {
        ExplanationContext {
            primary_age: 75,
            current_year: 2026,
            retirement_year: 2018,
            monte_carlo_success_rate: Some(0.87),
        }
    }

    #[tokio::test]
    async fn falls_back_to_template_when_provider_is_unset() {
        let config = Config {
            database_url: String::new(),
            jwt_secret: String::new(),
            jwt_expiry_hours: 24,
            host: String::new(),
            port: 0,
            plaid_client_id: None,
            plaid_secret: None,
            plaid_env: "sandbox".to_string(),
            ai_explanation_provider: "template".to_string(),
            anthropic_api_key: None,
            anthropic_model: "claude-opus-5".to_string(),
        };
        let result = explain_insight(&config, &sample_insight(), &sample_context())
            .await
            .unwrap();
        assert_eq!(result.provider, "template");
        assert!(result.explanation.contains("required minimum distribution"));
        assert!(result.explanation.contains("excise tax penalty"));
    }

    #[tokio::test]
    async fn falls_back_to_template_when_claude_requested_but_unconfigured() {
        let config = Config {
            database_url: String::new(),
            jwt_secret: String::new(),
            jwt_expiry_hours: 24,
            host: String::new(),
            port: 0,
            plaid_client_id: None,
            plaid_secret: None,
            plaid_env: "sandbox".to_string(),
            ai_explanation_provider: "claude".to_string(),
            anthropic_api_key: None,
            anthropic_model: "claude-opus-5".to_string(),
        };
        let result = explain_insight(&config, &sample_insight(), &sample_context())
            .await
            .unwrap();
        assert_eq!(result.provider, "template");
    }

    #[test]
    fn template_covers_every_insight_category() {
        let categories = [
            InsightCategory::AcaSubsidy,
            InsightCategory::Irmaa,
            InsightCategory::Rmd,
            InsightCategory::NegativeCashFlow,
            InsightCategory::UnexpectedSpending,
            InsightCategory::QuarterlyReviewDue,
            InsightCategory::AggressivePortfolio,
            InsightCategory::SequenceOfReturnRisk,
            InsightCategory::RegulatoryMilestone,
        ];
        let ctx = sample_context();
        for category in categories {
            let insight = Insight {
                category,
                severity: InsightSeverity::Low,
                title: "Title".to_string(),
                message: "Message".to_string(),
            };
            let text = template_explanation(&insight, &ctx);
            assert!(!text.is_empty());
        }
    }
}
