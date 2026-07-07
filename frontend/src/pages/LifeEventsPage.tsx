import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { LifeEvent, LifeEventRequest } from "../api/types";
import { LifeEventForm } from "../components/LifeEventForm";
import { Alert, Button, Card } from "../components/ui";
import { formatCurrency } from "../data/format";
import {
  directionLabel,
  lifeEventTypeLabel,
  recurrenceLabel,
} from "../data/lifeEvents";

type Editing = { mode: "new" } | { mode: "edit"; item: LifeEvent } | null;

export function LifeEventsPage() {
  const [items, setItems] = useState<LifeEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing>(null);

  async function refresh() {
    try {
      setItems(await api.listLifeEvents());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load life events");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSubmit(payload: LifeEventRequest) {
    if (editing?.mode === "edit") {
      await api.updateLifeEvent(editing.item.id, payload);
    } else {
      await api.createLifeEvent(payload);
    }
    setEditing(null);
    await refresh();
  }

  async function handleDelete(item: LifeEvent) {
    if (!window.confirm(`Delete "${item.name}"?`)) return;
    setError(null);
    try {
      await api.deleteLifeEvent(item.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  if (loading) return <p className="muted">Loading life events…</p>;

  // Net one-time cash impact across all events (recurring events count once here).
  const netImpact = items.reduce((sum, e) => sum + e.signed_amount, 0);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Life events</h1>
          <p className="muted">
            {items.length} event{items.length === 1 ? "" : "s"} · net{" "}
            <strong>{formatCurrency(netImpact)}</strong>
          </p>
        </div>
        {!editing && <Button onClick={() => setEditing({ mode: "new" })}>Add event</Button>}
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {editing && (
        <Card title={editing.mode === "edit" ? "Edit life event" : "New life event"}>
          <LifeEventForm
            initial={editing.mode === "edit" ? editing.item : undefined}
            onSubmit={handleSubmit}
            onCancel={() => setEditing(null)}
          />
        </Card>
      )}

      {items.length === 0 && !editing && (
        <Card>
          <p className="muted center">
            No life events yet. Add future events like selling a house, an
            inheritance, downsizing, or a large purchase.
          </p>
        </Card>
      )}

      {items.length > 0 && (
        <div className="account-list">
          {items.map((e) => (
            <div className="account-row" key={e.id}>
              <div className="account-main">
                <span className="account-name">{e.name}</span>
                <span className="account-meta muted">
                  {lifeEventTypeLabel(e.event_type)} · {directionLabel(e.direction)}
                  {e.recurrence !== "one_time"
                    ? ` · ${recurrenceLabel(e.recurrence)}`
                    : ""}
                  {e.taxable ? " · Taxable" : ""}
                  {e.inflation_adjusted ? " · Inflation-adjusted" : ""}
                </span>
                <span className="account-meta muted">
                  {e.event_date}
                  {e.recurrence !== "one_time" && e.end_date
                    ? ` to ${e.end_date}`
                    : ""}
                </span>
              </div>
              <div className="account-figures">
                <span
                  className="account-balance"
                  style={{ color: e.signed_amount < 0 ? "var(--danger, #b91c1c)" : undefined }}
                >
                  {formatCurrency(e.signed_amount)}
                </span>
                <span className="account-meta muted">
                  {e.recurrence === "one_time"
                    ? "one-time"
                    : `per ${e.recurrence === "monthly" ? "month" : "year"}`}
                </span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => setEditing({ mode: "edit", item: e })}>
                  Edit
                </Button>
                <Button variant="ghost" onClick={() => handleDelete(e)}>
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
