import type {
  LifeEventOccurrence,
  Projection,
  ProjectionSummary,
  YearProjection,
} from "../api/types";

export { formatCurrency, formatPercent, formatRate, formatSignedCurrency } from "./format";
export { categoryLabel } from "./accounts";

/**
 * Plain-language description of whether the plan's money lasts. Returns the
 * headline text plus a tone the UI maps to a colour.
 */
export function planOutlook(summary: ProjectionSummary): {
  text: string;
  tone: "good" | "warn";
} {
  if (summary.depletion_year != null) {
    return {
      text: `Funds run short in ${summary.depletion_year}`,
      tone: "warn",
    };
  }
  return { text: "Funded through end of plan", tone: "good" };
}

/**
 * Whether the projection contains any recommended withdrawals to show in the
 * quarterly schedule. When income covers spending there is nothing to withdraw.
 */
export function hasWithdrawals(projection: Projection): boolean {
  return projection.quarterly.some((q) => q.total_withdrawal > 0);
}

/** Net signed cash flow of a year's life events (inflows positive). */
export function lifeEventsNet(events: LifeEventOccurrence[]): number {
  return events.reduce((sum, e) => sum + e.amount, 0);
}

/**
 * Blended lifetime effective tax rate: total tax paid over the plan divided by
 * total gross income across all years (income plus each year's investment
 * growth). Returns a fraction (0–1), or 0 when there is no income to tax.
 */
export function lifetimeEffectiveRate(projection: Projection): number {
  const grossIncome = projection.annual.reduce(
    (sum, y) => sum + y.income + y.growth,
    0,
  );
  if (grossIncome <= 0) return 0;
  return projection.summary.total_lifetime_taxes / grossIncome;
}

/**
 * Tone for a year's life-event marker: "in" (green) when the net is an inflow,
 * "out" (red) when it is an outflow.
 */
export function lifeEventsTone(events: LifeEventOccurrence[]): "in" | "out" {
  return lifeEventsNet(events) < 0 ? "out" : "in";
}

/**
 * Dollar amount by which a year's required minimum distribution (RMD module)
 * exceeds that year's spending needs, or 0 when it doesn't. A positive result
 * means the household is forced to withdraw — and pay tax on — more than it
 * actually needs to spend that year.
 */
export function rmdExceedsSpendingBy(y: YearProjection): number {
  return Math.max(y.rmd_amount - y.spending, 0);
}
