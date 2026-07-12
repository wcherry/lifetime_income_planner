import type {
  SpendingTrackerCategory,
  SpendingTrackerCategoryKind,
  SpendingTrackerCategoryMonthSeries,
  SpendingTrackerMonth,
  SpendingTrackerTransaction,
} from "../api/types";

// Spending Tracker (transaction-level CSV import + categorization —
// distinct from the planned-budget "Spending" page) pure-logic helpers,
// mirroring the `data/quarterlyReview.ts` pattern so the live-preview logic
// used by `SpendingTrackerPage` and the Quarterly Review integration is
// unit-testable without hitting the API.

const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** [1,2,3] for quarter 1 ... [10,11,12] for quarter 4. Throws for quarter outside 1-4. */
export function monthsOfQuarter(quarter: number): number[] {
  switch (quarter) {
    case 1:
      return [1, 2, 3];
    case 2:
      return [4, 5, 6];
    case 3:
      return [7, 8, 9];
    case 4:
      return [10, 11, 12];
    default:
      throw new Error(`Quarter must be between 1 and 4, got ${quarter}`);
  }
}

export interface CategorizedTotals {
  incomeTotal: number;
  expenseTotal: number;
  ignoreTotal: number;
  uncategorizedCount: number;
}

/** Looks up each transaction's category by category_id in `categories`; sums abs(amount) into
 * incomeTotal/expenseTotal/ignoreTotal by the matched category's kind. A transaction with a null
 * category_id, or a category_id not found in `categories`, increments uncategorizedCount instead
 * and is excluded from all three totals. */
export function computeCategorizedTotals(
  transactions: SpendingTrackerTransaction[],
  categories: SpendingTrackerCategory[],
): CategorizedTotals {
  const categoriesById = new Map(categories.map((c) => [c.id, c]));
  const totals: CategorizedTotals = {
    incomeTotal: 0,
    expenseTotal: 0,
    ignoreTotal: 0,
    uncategorizedCount: 0,
  };

  for (const txn of transactions) {
    const category = txn.category_id ? categoriesById.get(txn.category_id) : undefined;
    if (!category) {
      totals.uncategorizedCount += 1;
      continue;
    }
    const amount = Math.abs(txn.amount);
    switch (category.kind) {
      case "income":
        totals.incomeTotal += amount;
        break;
      case "expense":
        totals.expenseTotal += amount;
        break;
      case "ignore":
        totals.ignoreTotal += amount;
        break;
    }
  }

  return totals;
}

export interface CategoryTotal {
  categoryId: string;
  categoryName: string;
  kind: SpendingTrackerCategoryKind;
  total: number;
  count: number;
}

/** Per-category breakdown: sums abs(amount) and counts transactions for each category that has at
 * least one matching transaction (categories with none are omitted). Transactions with a null or
 * unmatched category_id are excluded (see `computeCategorizedTotals` for the uncategorized count).
 * Sorted by total descending, ties broken alphabetically by category name. */
export function computeCategoryTotals(
  transactions: SpendingTrackerTransaction[],
  categories: SpendingTrackerCategory[],
): CategoryTotal[] {
  const categoriesById = new Map(categories.map((c) => [c.id, c]));
  const totalsById = new Map<string, CategoryTotal>();

  for (const txn of transactions) {
    const category = txn.category_id ? categoriesById.get(txn.category_id) : undefined;
    if (!category) continue;
    const existing = totalsById.get(category.id);
    if (existing) {
      existing.total += Math.abs(txn.amount);
      existing.count += 1;
    } else {
      totalsById.set(category.id, {
        categoryId: category.id,
        categoryName: category.name,
        kind: category.kind,
        total: Math.abs(txn.amount),
        count: 1,
      });
    }
  }

  return Array.from(totalsById.values()).sort(
    (a, b) => b.total - a.total || a.categoryName.localeCompare(b.categoryName),
  );
}

export interface MonthCoverageEntry {
  year: number;
  month: number;
  hasData: boolean;
}

/** For the 3 months of `quarter`/`year`, hasData is true iff a matching entry (by year+month) exists
 * in `months` (regardless of transaction_count > 0 vs 0 — presence in the list means it has been imported). */
export function quarterMonthCoverage(
  year: number,
  quarter: number,
  months: SpendingTrackerMonth[],
): MonthCoverageEntry[] {
  return monthsOfQuarter(quarter).map((month) => ({
    year,
    month,
    hasData: months.some((m) => m.year === year && m.month === month),
  }));
}

/** e.g. formatMonthLabel(2026, 1) -> "January 2026". */
export function formatMonthLabel(year: number, month: number): string {
  return `${MONTH_NAMES[month - 1]} ${year}`;
}

export interface YearChartSeries {
  id: string;
  name: string;
  monthlyTotals: number[];
  /** One of the app's 8 categorical CSS slots (`--cat-1`..`--cat-8`), or the
   * fixed muted color for the folded "Other" bucket. */
  color: string;
}

/** Number of `--cat-N` categorical CSS custom properties defined in app.css. */
export const YEAR_CHART_CATEGORICAL_SLOTS = 8;
export const YEAR_CHART_OTHER_ID = "__other__";
const YEAR_CHART_OTHER_COLOR = "var(--muted)";

/**
 * For the Spending Tracker's stacked year chart: folds any categories past
 * the eighth (by full-year total — `categories` already arrives sorted that
 * way from the API) into a single "Other" bucket, then assigns the eight
 * categorical hues by name — alphabetical, not by magnitude — so a series'
 * color stays fixed regardless of how the totals shift render to render
 * (color follows the entity, never its rank).
 */
export function buildYearChartSeries(
  categories: SpendingTrackerCategoryMonthSeries[] | undefined,
): YearChartSeries[] {
  if (!categories) return [];
  const overflowing = categories.length > YEAR_CHART_CATEGORICAL_SLOTS;
  const explicitCount = overflowing ? YEAR_CHART_CATEGORICAL_SLOTS - 1 : categories.length;
  const explicit = categories.slice(0, explicitCount);
  const overflow = categories.slice(explicitCount);

  const named = [...explicit].sort((a, b) => a.category_name.localeCompare(b.category_name));
  const series: YearChartSeries[] = named.map((c, i) => ({
    id: c.category_id,
    name: c.category_name,
    monthlyTotals: c.monthly_totals,
    color: `var(--cat-${i + 1})`,
  }));

  if (overflow.length > 0) {
    const monthlyTotals = Array.from({ length: 12 }, (_, m) =>
      overflow.reduce((sum, c) => sum + (c.monthly_totals[m] ?? 0), 0),
    );
    series.push({
      id: YEAR_CHART_OTHER_ID,
      name: "Other",
      monthlyTotals,
      color: YEAR_CHART_OTHER_COLOR,
    });
  }

  return series;
}
