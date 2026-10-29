import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { Task } from "../api/types";
import { Alert, Card } from "../components/ui";
import { CATEGORY_LABELS, groupTasksByCategory } from "../data/tasks";

/**
 * Task lists and reminders (roadmap Phase 5, feature 5): actionable to-dos
 * for the current quarter and year — take a withdrawal, pay estimated
 * taxes, convert to Roth, an RMD due, an ACA renewal window, an upcoming
 * regulatory deadline — derived from the same projection data as the
 * Insights page, but checkable so a user can track what's actually done.
 */
export function TasksPage() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pendingKey, setPendingKey] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        setTasks(await api.getTasks());
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load tasks");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  async function toggle(task: Task) {
    setError(null);
    setPendingKey(task.key);
    const nextCompleted = !task.completed;
    setTasks((ts) => ts.map((t) => (t.key === task.key ? { ...t, completed: nextCompleted } : t)));
    try {
      if (nextCompleted) {
        await api.completeTask(task.key);
      } else {
        await api.uncompleteTask(task.key);
      }
    } catch (err) {
      // Revert on failure.
      setTasks((ts) => ts.map((t) => (t.key === task.key ? { ...t, completed: task.completed } : t)));
      setError(err instanceof Error ? err.message : "Failed to update task");
    } finally {
      setPendingKey(null);
    }
  }

  if (loading) return <p className="muted">Loading tasks…</p>;

  const groups = groupTasksByCategory(tasks);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Tasks</h1>
          <p className="muted">
            What to actually do this quarter and year, generated from your plan — check items off
            as you complete them.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {groups.length === 0 && !error && (
        <Card>
          <p className="muted center">No tasks right now.</p>
        </Card>
      )}

      {groups.map((group) => (
        <Card title={CATEGORY_LABELS[group.category]} key={group.category}>
          <div className="account-list">
            {group.items.map((task) => (
              <label className="account-row checkbox-row" key={task.key}>
                <input
                  type="checkbox"
                  checked={task.completed}
                  disabled={pendingKey === task.key}
                  onChange={() => toggle(task)}
                />
                <div className="account-main">
                  <span className={`account-name${task.completed ? " muted" : ""}`}>
                    {task.title}
                  </span>
                  <span className="account-meta muted">{task.period}</span>
                  <span className="account-meta">{task.detail}</span>
                </div>
              </label>
            ))}
          </div>
        </Card>
      ))}
    </div>
  );
}
