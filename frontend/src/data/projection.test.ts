import {
  formatPercent,
  formatRate,
  formatSignedCurrency,
  hasWithdrawals,
  lifeEventsNet,
  lifeEventsTone,
  lifetimeEffectiveRate,
  planOutlook,
  rmdExceedsSpendingBy,
} from "./projection";
import type { Projection, ProjectionSummary, YearProjection, YearTax } from "../api/types";

const baseSummary: ProjectionSummary = {
  current_net_worth: 100000,
  projected_ending_balance: 50000,
  total_lifetime_income: 0,
  total_lifetime_spending: 0,
  total_lifetime_withdrawals: 0,
  total_lifetime_taxes: 0,
  total_lifetime_federal_taxes: 0,
  total_lifetime_state_taxes: 0,
  total_lifetime_roth_conversions: 0,
  depletion_year: null,
};

const emptyTax: YearTax = {
  ordinary_income: 0,
  qualified_dividends: 0,
  capital_gains: 0,
  social_security_benefits: 0,
  taxable_social_security: 0,
  adjusted_gross_income: 0,
  standard_deduction: 0,
  taxable_income: 0,
  federal_ordinary_tax: 0,
  federal_capital_gains_tax: 0,
  federal_tax: 0,
  state_taxable_income: 0,
  state_standard_deduction: 0,
  state_tax: 0,
  state_marginal_rate: 0,
  property_tax: 0,
  total_tax: 0,
  effective_rate: 0,
  marginal_rate: 0,
};

function year(overrides: Partial<YearProjection>): YearProjection {
  return {
    year: 2026,
    primary_age: 66,
    spouse_age: null,
    starting_balance: 0,
    income: 0,
    spending: 0,
    life_events_net: 0,
    life_events: [],
    milestones: [],
    growth: 0,
    withdrawals: 0,
    rmd_amount: 0,
    contributions: 0,
    roth_conversion: 0,
    taxes: 0,
    tax: emptyTax,
    withdrawal_order: "taxable_first",
    ending_balance: 0,
    shortfall: 0,
    ...overrides,
  };
}

function projectionWith(
  totalWithdrawals: number[],
  summary: ProjectionSummary = baseSummary,
  annual: YearProjection[] = [],
): Projection {
  return {
    current_year: 2026,
    start_year: 2026,
    end_year: 2050,
    assumptions: {
      inflation_rate: 2.5,
      investment_return_rate: 6,
      healthcare_inflation_rate: 4.5,
      social_security_cola_rate: 2,
      roth_conversion_ceiling: 0,
      roth_conversion_start_year: null,
      roth_conversion_end_year: null,
      withdrawal_strategy: "conventional",
      is_default: true,
    },
    summary,
    annual,
    quarterly: totalWithdrawals.map((total_withdrawal, i) => ({
      label: `2026 Q${i + 1}`,
      year: 2026,
      quarter: i + 1,
      income: 0,
      spending: 0,
      estimated_tax: 0,
      total_withdrawal,
      withdrawals: [],
    })),
    estimated_taxes: {
      tax_year: 2026,
      total: 0,
      note: "",
      payments: [],
    },
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

  it("formats a fractional rate as a percentage", () => {
    expect(formatRate(0.22)).toBe("22%");
    expect(formatRate(0.125)).toBe("12.5%");
    expect(formatRate(0)).toBe("0%");
  });

  it("blends the lifetime effective tax rate over income plus growth", () => {
    const summary: ProjectionSummary = { ...baseSummary, total_lifetime_taxes: 30000 };
    const annual = [
      year({ income: 60000, growth: 40000 }),
      year({ year: 2027, income: 60000, growth: 40000 }),
    ];
    // 30,000 tax / 200,000 gross = 15%.
    expect(lifetimeEffectiveRate(projectionWith([], summary, annual))).toBeCloseTo(0.15);
  });

  it("returns a zero lifetime rate when there is no income", () => {
    expect(lifetimeEffectiveRate(projectionWith([]))).toBe(0);
  });

  it("flags when a year's RMD exceeds its spending need", () => {
    expect(rmdExceedsSpendingBy(year({ rmd_amount: 9000, spending: 6000 }))).toBe(3000);
    expect(rmdExceedsSpendingBy(year({ rmd_amount: 6000, spending: 9000 }))).toBe(0);
    expect(rmdExceedsSpendingBy(year({ rmd_amount: 0, spending: 0 }))).toBe(0);
  });
});
