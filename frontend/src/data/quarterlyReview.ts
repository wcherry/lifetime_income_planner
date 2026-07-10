import type { ActualAccountBalanceInput } from "../api/types";

/** Client-side preview of the backend's reconciliation, mirroring backend/src/reconciliation.rs exactly. */
export interface ReconciliationPreview {
  total_starting_balance: number;
  total_ending_balance: number;
  net_balance_change: number;
  net_cash_flow: number;
  implied_investment_gain: number;
}

/**
 * Live preview of a quarter's reconciliation from in-progress form values, so
 * the user can see the implied investment gain/loss before submitting.
 * `startingBalances` and `enteredEndingBalances` are parallel arrays (one
 * entry per account) summed independently, not paired.
 */
export function previewReconciliation(
  startingBalances: number[],
  enteredEndingBalances: number[],
  actualIncome: number,
  actualSpending: number,
  actualTax: number,
): ReconciliationPreview {
  const total_starting_balance = startingBalances.reduce((sum, v) => sum + v, 0);
  const total_ending_balance = enteredEndingBalances.reduce((sum, v) => sum + v, 0);
  const net_balance_change = total_ending_balance - total_starting_balance;
  const net_cash_flow = actualIncome - actualSpending - actualTax;
  const implied_investment_gain = net_balance_change - net_cash_flow;

  return {
    total_starting_balance,
    total_ending_balance,
    net_balance_change,
    net_cash_flow,
    implied_investment_gain,
  };
}

/**
 * Whether every account in `accountIds` has a non-negative numeric entry in
 * `entered`, mirroring the backend's full-coverage and non-negative-balance
 * checks. Missing, blank, non-numeric, or negative entries all count as not
 * fully entered.
 */
export function allBalancesEntered(accountIds: string[], entered: Record<string, string>): boolean {
  return accountIds.every((id) => {
    const raw = entered[id];
    if (raw === undefined) return false;
    const trimmed = raw.trim();
    if (trimmed === "") return false;
    const value = Number(trimmed);
    return Number.isFinite(value) && value >= 0;
  });
}

/** Map entered form values into the request shape the complete-review endpoint expects. */
export function toActualBalances(entered: Record<string, string>): ActualAccountBalanceInput[] {
  return Object.entries(entered).map(([account_id, value]) => ({
    account_id,
    ending_balance: Number(value),
  }));
}
