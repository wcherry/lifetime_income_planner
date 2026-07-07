import { planSummary } from "./plans";
import type { PlanContents } from "../api/types";

const empty: PlanContents = {
  has_profile: false,
  has_assumptions: false,
  accounts: 0,
  income: 0,
  spending: 0,
  life_events: 0,
};

describe("planSummary", () => {
  it("describes an empty plan", () => {
    expect(planSummary(empty)).toBe("Empty plan");
  });

  it("lists only the sections that have content, pluralizing correctly", () => {
    expect(
      planSummary({
        ...empty,
        has_profile: true,
        has_assumptions: true,
        accounts: 3,
        income: 1,
      }),
    ).toBe("Profile · assumptions · 3 accounts · 1 income source");
  });

  it("omits zero-count sections", () => {
    expect(planSummary({ ...empty, accounts: 1, life_events: 2 })).toBe(
      "1 account · 2 life events",
    );
  });
});
