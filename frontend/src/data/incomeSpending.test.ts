import {
  incomeOwnerLabel,
  incomeTypeLabel,
  taxabilityLabel,
} from "./income";
import {
  spendingCategoryLabel,
  spendingFrequencyLabel,
} from "./spending";
import {
  directionLabel,
  lifeEventTypeLabel,
  recurrenceLabel,
} from "./lifeEvents";

describe("spending helpers", () => {
  it("labels categories and frequencies", () => {
    expect(spendingCategoryLabel("home_maintenance")).toBe("Home maintenance");
    expect(spendingCategoryLabel("one_time")).toBe("One-time expense");
    expect(spendingFrequencyLabel("annual")).toBe("Annual");
  });
});

describe("income helpers", () => {
  it("labels types, taxability, and owners", () => {
    expect(incomeTypeLabel("social_security")).toBe("Social Security");
    expect(incomeTypeLabel("part_time")).toBe("Part-time work");
    expect(taxabilityLabel("partially_taxable")).toBe("Partially taxable");
    expect(incomeOwnerLabel("joint")).toBe("Joint");
  });
});

describe("life event helpers", () => {
  it("labels event types, directions, and recurrence", () => {
    expect(lifeEventTypeLabel("sell_house")).toBe("Sell house");
    expect(lifeEventTypeLabel("death_of_spouse")).toBe("Death of spouse");
    expect(directionLabel("inflow")).toBe("Money in");
    expect(directionLabel("outflow")).toBe("Money out");
    expect(recurrenceLabel("one_time")).toBe("One-time");
    expect(recurrenceLabel("annual")).toBe("Annual");
  });
});
