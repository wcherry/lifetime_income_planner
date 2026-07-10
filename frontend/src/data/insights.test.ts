import type { Insight } from "../api/types";
import { groupInsightsBySeverity } from "./insights";

function insight(severity: Insight["severity"], title: string): Insight {
  return { category: "rmd", severity, title, message: title };
}

describe("groupInsightsBySeverity", () => {
  it("orders groups highest severity first", () => {
    const groups = groupInsightsBySeverity([
      insight("low", "a"),
      insight("high", "b"),
      insight("medium", "c"),
    ]);
    expect(groups.map((g) => g.severity)).toEqual(["high", "medium", "low"]);
  });

  it("omits severities with no insights", () => {
    const groups = groupInsightsBySeverity([insight("high", "b")]);
    expect(groups).toHaveLength(1);
    expect(groups[0].severity).toBe("high");
  });

  it("keeps all insights within a severity group", () => {
    const groups = groupInsightsBySeverity([insight("high", "a"), insight("high", "b")]);
    expect(groups[0].items.map((i) => i.title)).toEqual(["a", "b"]);
  });

  it("returns no groups for an empty list", () => {
    expect(groupInsightsBySeverity([])).toEqual([]);
  });
});
