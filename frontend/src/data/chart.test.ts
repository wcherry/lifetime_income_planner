import { axisTicks } from "./chart";
import { formatCompactCurrency } from "./format";

describe("axisTicks", () => {
  it("covers the data max with evenly spaced ticks starting at 0", () => {
    const { niceMax, ticks } = axisTicks(920000, 4);
    expect(ticks[0]).toBe(0);
    expect(niceMax).toBeGreaterThanOrEqual(920000);
    expect(ticks[ticks.length - 1]).toBe(niceMax);
    // Evenly spaced.
    const step = ticks[1] - ticks[0];
    for (let i = 1; i < ticks.length; i++) {
      expect(ticks[i] - ticks[i - 1]).toBe(step);
    }
  });

  it("handles a zero or negative max gracefully", () => {
    expect(axisTicks(0)).toEqual({ niceMax: 0, ticks: [0] });
    expect(axisTicks(-5)).toEqual({ niceMax: 0, ticks: [0] });
  });
});

describe("formatCompactCurrency", () => {
  it("compacts thousands and millions", () => {
    expect(formatCompactCurrency(250000)).toBe("$250K");
    expect(formatCompactCurrency(1200000)).toBe("$1.2M");
    expect(formatCompactCurrency(12000000)).toBe("$12M");
    expect(formatCompactCurrency(0)).toBe("$0");
  });
});
