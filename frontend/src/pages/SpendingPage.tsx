import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { SpendingItem, SpendingRequest } from "../api/types";
import { SpendingForm } from "../components/SpendingForm";
import { Alert, Button, Card } from "../components/ui";
import { formatCurrency } from "../data/format";
import { spendingCategoryLabel, spendingFrequencyLabel } from "../data/spending";

type Editing = { mode: "new" } | { mode: "edit"; item: SpendingItem } | null;

export function SpendingPage() {
  const [items, setItems] = useState<SpendingItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing>(null);

  async function refresh() {
    try {
      setItems(await api.listSpending());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load spending");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSubmit(payload: SpendingRequest) {
    if (editing?.mode === "edit") {
      await api.updateSpending(editing.item.id, payload);
    } else {
      await api.createSpending(payload);
    }
    setEditing(null);
    await refresh();
  }

  async function handleDelete(item: SpendingItem) {
    if (!window.confirm(`Delete "${item.name}"?`)) return;
    setError(null);
    try {
      await api.deleteSpending(item.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  if (loading) return <p className="muted">Loading spending plan…</p>;

  // Recurring annual total excludes one-time expenses.
  const recurringTotal = items
    .filter((i) => i.frequency !== "one_time")
    .reduce((sum, i) => sum + i.annual_amount, 0);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Spending plan</h1>
          <p className="muted">
            {items.length} item{items.length === 1 ? "" : "s"} ·{" "}
            <strong>{formatCurrency(recurringTotal)}</strong>/yr recurring
          </p>
        </div>
        {!editing && <Button onClick={() => setEditing({ mode: "new" })}>Add expense</Button>}
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {editing && (
        <Card title={editing.mode === "edit" ? "Edit expense" : "New expense"}>
          <SpendingForm
            initial={editing.mode === "edit" ? editing.item : undefined}
            onSubmit={handleSubmit}
            onCancel={() => setEditing(null)}
          />
        </Card>
      )}

      {items.length === 0 && !editing && (
        <Card>
          <p className="muted center">
            No expenses yet. Add your essential, discretionary, healthcare, and
            travel spending to build your plan.
          </p>
        </Card>
      )}

      {items.length > 0 && (
        <div className="account-list">
          {items.map((i) => (
            <div className="account-row" key={i.id}>
              <div className="account-main">
                <span className="account-name">{i.name}</span>
                <span className="account-meta muted">
                  {spendingCategoryLabel(i.category)} ·{" "}
                  {spendingFrequencyLabel(i.frequency)}
                  {i.inflation_adjusted ? " · inflation-adjusted" : ""}
                  {i.start_year || i.end_year
                    ? ` · ${i.start_year ?? "…"}–${i.end_year ?? "…"}`
                    : ""}
                </span>
              </div>
              <div className="account-figures">
                <span className="account-balance">{formatCurrency(i.amount)}</span>
                <span className="account-meta muted">
                  {i.frequency === "one_time"
                    ? "one-time"
                    : `${formatCurrency(i.annual_amount)}/yr`}
                </span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => setEditing({ mode: "edit", item: i })}>
                  Edit
                </Button>
                <Button variant="ghost" onClick={() => handleDelete(i)}>
                  Delete
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
