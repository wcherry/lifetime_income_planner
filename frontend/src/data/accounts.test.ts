import {
  accountTypeLabel,
  categoryLabel,
  formatCurrency,
  ownerLabel,
  TYPES_BY_CATEGORY,
} from "./accounts";

describe("account helpers", () => {
  it("formats currency without cents", () => {
    expect(formatCurrency(250000)).toBe("$250,000");
    expect(formatCurrency(0)).toBe("$0");
  });

  it("labels categories, types, and owners", () => {
    expect(categoryLabel("tax_deferred")).toBe("Tax-deferred");
    expect(accountTypeLabel("401k")).toBe("401(k)");
    expect(accountTypeLabel("roth_ira")).toBe("Roth IRA");
    expect(ownerLabel("joint")).toBe("Joint");
  });

  it("maps every category to at least one account type", () => {
    for (const types of Object.values(TYPES_BY_CATEGORY)) {
      expect(types.length).toBeGreaterThan(0);
    }
  });
});
