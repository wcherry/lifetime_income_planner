import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { IncomeRequest, IncomeSource } from "../api/types";
import { IncomeForm } from "../components/IncomeForm";
import { Alert, Button, Card } from "../components/ui";
import { formatCurrency } from "../data/format";
import { incomeOwnerLabel, incomeTypeLabel, taxabilityLabel } from "../data/income";

type Editing = { mode: "new" } | { mode: "edit"; item: IncomeSource } | null;

export function IncomePage() {
  const [items, setItems] = useState<IncomeSource[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing>(null);

  async function refresh() {
    try {
      setItems(await api.listIncome());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load income");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSubmit(payload: IncomeRequest) {
    if (editing?.mode === "edit") {
      await api.updateIncome(editing.item.id, payload);
    } else {
      await api.createIncome(payload);
    }
    setEditing(null);
    await refresh();
  }

  async function handleDelete(item: IncomeSource) {
    if (!window.confirm(`Delete "${item.name}"?`)) return;
    setError(null);
    try {
      await api.deleteIncome(item.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  if (loading) return <p className="muted">Loading income sources…</p>;

  const annualTotal = items.reduce((sum, i) => sum + i.annual_amount, 0);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Income sources</h1>
          <p className="muted">
            {items.length} source{items.length === 1 ? "" : "s"} ·{" "}
            <strong>{formatCurrency(annualTotal)}</strong>/yr
          </p>
        </div>
        {!editing && <Button onClick={() => setEditing({ mode: "new" })}>Add income</Button>}
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {editing && (
        <Card title={editing.mode === "edit" ? "Edit income" : "New income"}>
          <IncomeForm
            initial={editing.mode === "edit" ? editing.item : undefined}
            onSubmit={handleSubmit}
            onCancel={() => setEditing(null)}
          />
        </Card>
      )}

      {items.length === 0 && !editing && (
        <Card>
          <p className="muted center">
            No income sources yet. Add Social Security, pensions, annuities, and any
            employment or consulting income.
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
                  {incomeTypeLabel(i.income_type)} · {incomeOwnerLabel(i.owner)} ·{" "}
                  {taxabilityLabel(i.taxability)}
                  {i.cola ? " · COLA" : ""}
                  {i.growth_rate ? ` · ${i.growth_rate}%/yr` : ""}
                </span>
                <span className="account-meta muted">
                  From {i.start_date}
                  {i.end_date ? ` to ${i.end_date}` : " (for life)"}
                </span>
              </div>
              <div className="account-figures">
                <span className="account-balance">{formatCurrency(i.annual_amount)}</span>
                <span className="account-meta muted">per year</span>
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
