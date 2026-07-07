import type { LifeEventOccurrence, Projection, ProjectionSummary } from "../api/types";

export { formatCurrency, formatPercent, formatSignedCurrency } from "./format";
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
 * Tone for a year's life-event marker: "in" (green) when the net is an inflow,
 * "out" (red) when it is an outflow.
 */
export function lifeEventsTone(events: LifeEventOccurrence[]): "in" | "out" {
  return lifeEventsNet(events) < 0 ? "out" : "in";
}
