import type { Insight, InsightCategory, InsightSeverity } from "../api/types";

export const SEVERITY_LABELS: Record<InsightSeverity, string> = {
  high: "High priority",
  medium: "Medium priority",
  low: "Low priority",
};

export const CATEGORY_LABELS: Record<InsightCategory, string> = {
  aca_subsidy: "ACA subsidy",
  irmaa: "Medicare IRMAA",
  rmd: "Required minimum distribution",
  negative_cash_flow: "Cash flow",
  unexpected_spending: "Spending",
  quarterly_review_due: "Quarterly review",
  aggressive_portfolio: "Portfolio allocation",
  sequence_of_return_risk: "Sequence-of-return risk",
};

const SEVERITY_ORDER: InsightSeverity[] = ["high", "medium", "low"];

export interface InsightGroup {
  severity: InsightSeverity;
  items: Insight[];
}

/** Group insights by severity, highest first, omitting empty groups. */
export function groupInsightsBySeverity(insights: Insight[]): InsightGroup[] {
  return SEVERITY_ORDER.map((severity) => ({
    severity,
    items: insights.filter((i) => i.severity === severity),
  })).filter((group) => group.items.length > 0);
}
