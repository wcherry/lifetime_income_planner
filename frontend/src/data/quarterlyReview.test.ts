import { allBalancesEntered, previewReconciliation, toActualBalances } from "./quarterlyReview";

describe("previewReconciliation", () => {
  // Same figures as backend/src/reconciliation.rs tests, for parity.
  const actualIncome = 10_000;
  const actualSpending = 6_000;
  const actualTax = 1_000;

  it("shows zero implied gain when balance change equals net cash flow", () => {
    const result = previewReconciliation(
      [100_000],
      [103_000],
      actualIncome,
      actualSpending,
      actualTax,
    );
    expect(result.total_starting_balance).toBe(100_000);
    expect(result.total_ending_balance).toBe(103_000);
    expect(result.net_balance_change).toBe(3_000);
    expect(result.net_cash_flow).toBe(3_000);
    expect(result.implied_investment_gain).toBe(0);
  });

  it("shows a positive implied investment gain when balances grew more than cash flow explains", () => {
    const result = previewReconciliation(
      [100_000],
      [103_500],
      actualIncome,
      actualSpending,
      actualTax,
    );
    expect(result.net_balance_change).toBe(3_500);
    expect(result.net_cash_flow).toBe(3_000);
    expect(result.implied_investment_gain).toBe(500);
  });

  it("shows a negative implied investment gain (a loss) on a shortfall", () => {
    const result = previewReconciliation(
      [100_000],
      [101_000],
      actualIncome,
      actualSpending,
      actualTax,
    );
    expect(result.net_balance_change).toBe(1_000);
    expect(result.net_cash_flow).toBe(3_000);
    expect(result.implied_investment_gain).toBe(-2_000);
  });

  it("sums parallel starting/ending balance arrays independently across multiple accounts", () => {
    const result = previewReconciliation(
      [100_000, 50_000],
      [103_000, 51_000],
      actualIncome,
      actualSpending,
      actualTax,
    );
    expect(result.total_starting_balance).toBe(150_000);
    expect(result.total_ending_balance).toBe(154_000);
  });
});

describe("allBalancesEntered", () => {
  it("is false when an account id is missing from the entered map", () => {
    expect(allBalancesEntered(["a1", "a2"], { a1: "1000" })).toBe(false);
  });

  it("is false when an entry is non-numeric", () => {
    expect(allBalancesEntered(["a1"], { a1: "abc" })).toBe(false);
  });

  it("is false when an entry is negative", () => {
    expect(allBalancesEntered(["a1"], { a1: "-5" })).toBe(false);
  });

  it("is false when an entry is blank or whitespace-only", () => {
    expect(allBalancesEntered(["a1"], { a1: "   " })).toBe(false);
  });

  it("is true when every account id has a non-negative numeric entry", () => {
    expect(allBalancesEntered(["a1", "a2"], { a1: "1000.50", a2: "0" })).toBe(true);
  });
});

describe("toActualBalances", () => {
  it("maps and coerces every entered value", () => {
    const result = toActualBalances({ a1: "1000.50", a2: "0" });
    expect(result).toHaveLength(2);
    expect(result).toContainEqual({ account_id: "a1", ending_balance: 1000.5 });
    expect(result).toContainEqual({ account_id: "a2", ending_balance: 0 });
  });
});
