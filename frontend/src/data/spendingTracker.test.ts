import type {
  SpendingTrackerCategory,
  SpendingTrackerCategoryMonthSeries,
  SpendingTrackerMonth,
  SpendingTrackerTransaction,
} from "../api/types";
import {
  buildYearChartSeries,
  computeCategorizedTotals,
  computeCategoryTotals,
  formatMonthLabel,
  monthsOfQuarter,
  quarterMonthCoverage,
} from "./spendingTracker";

function buildCategorySeries(
  overrides: Partial<SpendingTrackerCategoryMonthSeries> = {},
): SpendingTrackerCategoryMonthSeries {
  return {
    category_id: "cat-1",
    category_name: "Groceries",
    monthly_totals: new Array(12).fill(0),
    ...overrides,
  };
}

function buildCategory(overrides: Partial<SpendingTrackerCategory> = {}): SpendingTrackerCategory {
  return {
    id: "cat-1",
    is_own: false,
    name: "Groceries",
    kind: "expense",
    is_predefined: true,
    ...overrides,
  };
}

function buildTransaction(
  overrides: Partial<SpendingTrackerTransaction> = {},
): SpendingTrackerTransaction {
  return {
    id: "txn-1",
    year: 2026,
    month: 1,
    transaction_date: "2026-01-05",
    description: "Coffee Shop",
    amount: -4.5,
    category_id: "cat-1",
    category_name: "Groceries",
    category_kind: "expense",
    raw_row: { Date: "2026-01-05", Description: "Coffee Shop", Amount: "-4.50" },
    ...overrides,
  };
}

describe("monthsOfQuarter", () => {
  it("returns January-March for quarter 1", () => {
    expect(monthsOfQuarter(1)).toEqual([1, 2, 3]);
  });

  it("returns April-June for quarter 2", () => {
    expect(monthsOfQuarter(2)).toEqual([4, 5, 6]);
  });

  it("returns July-September for quarter 3", () => {
    expect(monthsOfQuarter(3)).toEqual([7, 8, 9]);
  });

  it("returns October-December for quarter 4", () => {
    expect(monthsOfQuarter(4)).toEqual([10, 11, 12]);
  });

  it("throws for quarter 0", () => {
    expect(() => monthsOfQuarter(0)).toThrow();
  });

  it("throws for quarter 5", () => {
    expect(() => monthsOfQuarter(5)).toThrow();
  });
});

describe("computeCategorizedTotals", () => {
  it("sums mixed income/expense/ignore categories using absolute values", () => {
    const categories = [
      buildCategory({ id: "income-cat", kind: "income" }),
      buildCategory({ id: "expense-cat", kind: "expense" }),
      buildCategory({ id: "ignore-cat", kind: "ignore" }),
    ];
    const transactions = [
      buildTransaction({ id: "t1", category_id: "income-cat", amount: 2000 }),
      // A negative-signed expense amount should still contribute positively to expenseTotal.
      buildTransaction({ id: "t2", category_id: "expense-cat", amount: -150.25 }),
      buildTransaction({ id: "t3", category_id: "ignore-cat", amount: -75 }),
    ];

    const result = computeCategorizedTotals(transactions, categories);

    expect(result.incomeTotal).toBe(2000);
    expect(result.expenseTotal).toBe(150.25);
    expect(result.ignoreTotal).toBe(75);
    expect(result.uncategorizedCount).toBe(0);
  });

  it("counts a null category_id as uncategorized and excludes it from totals", () => {
    const categories = [buildCategory({ id: "expense-cat", kind: "expense" })];
    const transactions = [
      buildTransaction({ id: "t1", category_id: null, amount: -50 }),
      buildTransaction({ id: "t2", category_id: "expense-cat", amount: -25 }),
    ];

    const result = computeCategorizedTotals(transactions, categories);

    expect(result.uncategorizedCount).toBe(1);
    expect(result.expenseTotal).toBe(25);
  });

  it("counts a category_id with no matching category as uncategorized", () => {
    const categories = [buildCategory({ id: "expense-cat", kind: "expense" })];
    const transactions = [
      buildTransaction({ id: "t1", category_id: "does-not-exist", amount: -50 }),
    ];

    const result = computeCategorizedTotals(transactions, categories);

    expect(result.uncategorizedCount).toBe(1);
    expect(result.incomeTotal).toBe(0);
    expect(result.expenseTotal).toBe(0);
    expect(result.ignoreTotal).toBe(0);
  });

  it("excludes ignore-kind transactions from totals without counting them as uncategorized", () => {
    const categories = [buildCategory({ id: "ignore-cat", kind: "ignore" })];
    const transactions = [buildTransaction({ id: "t1", category_id: "ignore-cat", amount: -75 })];

    const result = computeCategorizedTotals(transactions, categories);

    expect(result.ignoreTotal).toBe(75);
    expect(result.uncategorizedCount).toBe(0);
  });
});

describe("computeCategoryTotals", () => {
  it("sums abs(amount) and counts per category, omitting categories with no transactions", () => {
    const categories = [
      buildCategory({ id: "groceries", name: "Groceries", kind: "expense" }),
      buildCategory({ id: "unused", name: "Unused Category", kind: "expense" }),
    ];
    const transactions = [
      buildTransaction({ id: "t1", category_id: "groceries", amount: -40 }),
      buildTransaction({ id: "t2", category_id: "groceries", amount: -10.5 }),
    ];

    const result = computeCategoryTotals(transactions, categories);

    expect(result).toEqual([
      {
        categoryId: "groceries",
        categoryName: "Groceries",
        kind: "expense",
        total: 50.5,
        count: 2,
      },
    ]);
  });

  it("excludes null and unmatched category_id transactions", () => {
    const categories = [buildCategory({ id: "groceries", name: "Groceries", kind: "expense" })];
    const transactions = [
      buildTransaction({ id: "t1", category_id: null, amount: -40 }),
      buildTransaction({ id: "t2", category_id: "does-not-exist", amount: -10 }),
    ];

    expect(computeCategoryTotals(transactions, categories)).toEqual([]);
  });

  it("sorts by total descending, ties broken alphabetically by category name", () => {
    const categories = [
      buildCategory({ id: "b", name: "Bravo", kind: "expense" }),
      buildCategory({ id: "a", name: "Alpha", kind: "expense" }),
      buildCategory({ id: "c", name: "Charlie", kind: "income" }),
    ];
    const transactions = [
      buildTransaction({ id: "t1", category_id: "a", amount: -20 }),
      buildTransaction({ id: "t2", category_id: "b", amount: -20 }),
      buildTransaction({ id: "t3", category_id: "c", amount: 100 }),
    ];

    const result = computeCategoryTotals(transactions, categories);

    expect(result.map((r) => r.categoryId)).toEqual(["c", "a", "b"]);
  });
});

describe("quarterMonthCoverage", () => {
  it("marks only the month present in `months` as hasData in a partial quarter", () => {
    const months: SpendingTrackerMonth[] = [
      { year: 2026, month: 2, transaction_count: 12, last_imported_at: "2026-02-15T00:00:00Z" },
    ];

    const result = quarterMonthCoverage(2026, 1, months);

    expect(result).toEqual([
      { year: 2026, month: 1, hasData: false },
      { year: 2026, month: 2, hasData: true },
      { year: 2026, month: 3, hasData: false },
    ]);
  });

  it("treats a month present with zero transaction_count as still having data (it was imported)", () => {
    const months: SpendingTrackerMonth[] = [
      { year: 2026, month: 1, transaction_count: 0, last_imported_at: "2026-01-15T00:00:00Z" },
    ];

    const result = quarterMonthCoverage(2026, 1, months);

    const jan = result.find((m) => m.month === 1);
    expect(jan?.hasData).toBe(true);
  });
});

describe("buildYearChartSeries", () => {
  it("assigns categorical colors in alphabetical order, not by magnitude", () => {
    const categories = [
      buildCategorySeries({ category_id: "housing", category_name: "Housing" }),
      buildCategorySeries({ category_id: "food", category_name: "Food" }),
    ];

    const result = buildYearChartSeries(categories);

    expect(result.map((s) => s.id)).toEqual(["food", "housing"]);
    expect(result[0].color).toBe("var(--cat-1)");
    expect(result[1].color).toBe("var(--cat-2)");
  });

  it("passes through monthly totals unchanged for a category within the first 8", () => {
    const monthly = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40];
    const categories = [buildCategorySeries({ monthly_totals: monthly })];

    const result = buildYearChartSeries(categories);

    expect(result).toHaveLength(1);
    expect(result[0].monthlyTotals).toEqual(monthly);
  });

  it("folds the 8th-and-beyond categories into a single muted 'Other' series", () => {
    // 9 categories, already sorted by total descending as the API returns them.
    const categories = Array.from({ length: 9 }, (_, i) =>
      buildCategorySeries({
        category_id: `cat-${i}`,
        category_name: `Category ${i}`,
        monthly_totals: new Array(12).fill(9 - i), // descending totals
      }),
    );

    const result = buildYearChartSeries(categories);

    // 7 explicit categories + 1 "Other" bucket = 8 series, never more.
    expect(result).toHaveLength(8);
    const other = result.find((s) => s.name === "Other");
    expect(other).toBeDefined();
    expect(other?.color).toBe("var(--muted)");
    // The two lowest-total categories (index 7 and 8, totals 2 and 1) are folded.
    expect(other?.monthlyTotals[0]).toBe(2 + 1);
    // The folded categories are gone as their own series.
    expect(result.some((s) => s.id === "cat-7")).toBe(false);
    expect(result.some((s) => s.id === "cat-8")).toBe(false);
  });

  it("returns an empty array for no categories", () => {
    expect(buildYearChartSeries([])).toEqual([]);
  });
});

describe("formatMonthLabel", () => {
  it("formats January 2026", () => {
    expect(formatMonthLabel(2026, 1)).toBe("January 2026");
  });

  it("formats December 2025", () => {
    expect(formatMonthLabel(2025, 12)).toBe("December 2025");
  });
});
