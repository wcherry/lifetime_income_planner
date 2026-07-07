import {
  formatPercent,
  formatSignedCurrency,
  hasWithdrawals,
  lifeEventsNet,
  lifeEventsTone,
  planOutlook,
} from "./projection";
import type { Projection, ProjectionSummary } from "../api/types";

const baseSummary: ProjectionSummary = {
  current_net_worth: 100000,
  projected_ending_balance: 50000,
  total_lifetime_income: 0,
  total_lifetime_spending: 0,
  total_lifetime_withdrawals: 0,
  depletion_year: null,
};

function projectionWith(totalWithdrawals: number[]): Projection {
  return {
    current_year: 2026,
    start_year: 2026,
    end_year: 2050,
    assumptions: {
      inflation_rate: 2.5,
      investment_return_rate: 6,
      healthcare_inflation_rate: 4.5,
      social_security_cola_rate: 2,
      is_default: true,
    },
    summary: baseSummary,
    annual: [],
    quarterly: totalWithdrawals.map((total_withdrawal, i) => ({
      label: `2026 Q${i + 1}`,
      year: 2026,
      quarter: i + 1,
      income: 0,
      spending: 0,
      total_withdrawal,
      withdrawals: [],
    })),
  };
}

describe("projection helpers", () => {
  it("formats percentages to one decimal", () => {
    expect(formatPercent(2.5)).toBe("2.5%");
    expect(formatPercent(6)).toBe("6.0%");
  });

  it("reports a good outlook when funds last", () => {
    expect(planOutlook(baseSummary)).toEqual({
      text: "Funded through end of plan",
      tone: "good",
    });
  });

  it("warns with the depletion year when funds run short", () => {
    const outlook = planOutlook({ ...baseSummary, depletion_year: 2041 });
    expect(outlook.tone).toBe("warn");
    expect(outlook.text).toContain("2041");
  });

  it("detects whether any quarter has a withdrawal", () => {
    expect(hasWithdrawals(projectionWith([0, 0, 0, 0]))).toBe(false);
    expect(hasWithdrawals(projectionWith([1000, 1000, 1000, 1000]))).toBe(true);
  });

  it("signs currency with an explicit + or −", () => {
    expect(formatSignedCurrency(200000)).toBe("+$200,000");
    expect(formatSignedCurrency(-80000)).toBe("−$80,000");
  });

  it("nets life events and picks a tone from the net", () => {
    const events = [
      { name: "Inheritance", amount: 200000 },
      { name: "Buy RV", amount: -80000 },
    ];
    expect(lifeEventsNet(events)).toBe(120000);
    expect(lifeEventsTone(events)).toBe("in");
    expect(lifeEventsTone([{ name: "Buy RV", amount: -80000 }])).toBe("out");
  });
});
