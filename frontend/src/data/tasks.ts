import type { Task, TaskCategory } from "../api/types";

export const CATEGORY_LABELS: Record<TaskCategory, string> = {
  withdrawal: "Withdrawal",
  estimated_tax: "Estimated tax",
  roth_conversion: "Roth conversion",
  rmd: "Required minimum distribution",
  aca_renewal: "ACA renewal",
  regulatory: "Regulatory deadline",
};

const CATEGORY_ORDER: TaskCategory[] = [
  "regulatory",
  "rmd",
  "estimated_tax",
  "withdrawal",
  "roth_conversion",
  "aca_renewal",
];

/** Incomplete tasks first, then completed; otherwise keeps input order. */
export function sortTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => Number(a.completed) - Number(b.completed));
}

export interface TaskGroup {
  category: TaskCategory;
  items: Task[];
}

/**
 * Group tasks by category in a fixed, sensible display order (most urgent
 * categories first), omitting empty groups. Within each group, incomplete
 * tasks sort before completed ones.
 */
export function groupTasksByCategory(tasks: Task[]): TaskGroup[] {
  return CATEGORY_ORDER.map((category) => ({
    category,
    items: sortTasks(tasks.filter((t) => t.category === category)),
  })).filter((group) => group.items.length > 0);
}
