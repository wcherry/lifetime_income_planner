import type { Task } from "../api/types";
import { groupTasksByCategory, sortTasks } from "./tasks";

function task(category: Task["category"], key: string, completed = false): Task {
  return { key, category, title: key, detail: key, period: "2026", completed };
}

describe("sortTasks", () => {
  it("puts incomplete tasks before completed ones", () => {
    const sorted = sortTasks([
      task("rmd", "done", true),
      task("rmd", "pending", false),
    ]);
    expect(sorted.map((t) => t.key)).toEqual(["pending", "done"]);
  });
});

describe("groupTasksByCategory", () => {
  it("orders groups with regulatory first and omits empty categories", () => {
    const groups = groupTasksByCategory([
      task("aca_renewal", "a"),
      task("regulatory", "b"),
      task("rmd", "c"),
    ]);
    expect(groups.map((g) => g.category)).toEqual(["regulatory", "rmd", "aca_renewal"]);
  });

  it("keeps all tasks within a category group, incomplete first", () => {
    const groups = groupTasksByCategory([
      task("rmd", "a", true),
      task("rmd", "b", false),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].items.map((t) => t.key)).toEqual(["b", "a"]);
  });

  it("returns no groups for an empty list", () => {
    expect(groupTasksByCategory([])).toEqual([]);
  });
});
