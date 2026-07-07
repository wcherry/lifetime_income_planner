import type { PlanContents } from "../api/types";

function plural(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

/**
 * A compact one-line description of what a saved plan holds, e.g.
 * "Profile · assumptions · 3 accounts · 2 income sources". Sections with
 * nothing in them are omitted; an empty plan reads "Empty plan".
 */
export function planSummary(contents: PlanContents): string {
  const parts: string[] = [];
  if (contents.has_profile) parts.push("Profile");
  if (contents.has_assumptions) parts.push("assumptions");
  if (contents.accounts > 0) parts.push(plural(contents.accounts, "account"));
  if (contents.income > 0) parts.push(plural(contents.income, "income source"));
  if (contents.spending > 0) parts.push(plural(contents.spending, "spending item"));
  if (contents.life_events > 0) parts.push(plural(contents.life_events, "life event"));
  return parts.length > 0 ? parts.join(" · ") : "Empty plan";
}
