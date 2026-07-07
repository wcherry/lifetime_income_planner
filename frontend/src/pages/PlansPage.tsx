import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { Plan } from "../api/types";
import { Alert, Button, Card, Field, TextInput } from "../components/ui";
import { planSummary } from "../data/plans";

function formatDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

export function PlansPage() {
  const [plans, setPlans] = useState<Plan[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);

  async function refresh() {
    try {
      setPlans(await api.listPlans());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load plans");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSave(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setNotice(null);
    setSaving(true);
    try {
      const plan = await api.savePlan({ name: name.trim() });
      setName("");
      setNotice(`Saved "${plan.name}".`);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save plan");
    } finally {
      setSaving(false);
    }
  }

  async function handleLoad(plan: Plan) {
    if (
      !window.confirm(
        `Load "${plan.name}"? This replaces your current profile, accounts, income, ` +
          `spending, life events, and assumptions with this saved plan.`,
      )
    )
      return;
    setError(null);
    setNotice(null);
    setBusyId(plan.id);
    try {
      await api.loadPlan(plan.id);
      setNotice(`Loaded "${plan.name}" into your working plan.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load plan");
    } finally {
      setBusyId(null);
    }
  }

  async function handleRename(plan: Plan) {
    const next = window.prompt("Rename plan", plan.name);
    if (next == null) return;
    const trimmed = next.trim();
    if (trimmed === "" || trimmed === plan.name) return;
    setError(null);
    setNotice(null);
    try {
      await api.renamePlan(plan.id, { name: trimmed });
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to rename plan");
    }
  }

  async function handleDelete(plan: Plan) {
    if (!window.confirm(`Delete saved plan "${plan.name}"? This cannot be undone.`)) return;
    setError(null);
    setNotice(null);
    try {
      await api.deletePlan(plan.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete plan");
    }
  }

  if (loading) return <p className="muted">Loading saved plans…</p>;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Saved plans</h1>
          <p className="muted">
            Snapshot your whole plan and switch between versions. Loading a plan replaces your
            current working data.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}
      {notice && <Alert kind="success">{notice}</Alert>}

      <Card title="Save current plan">
        <form onSubmit={handleSave}>
          <Field
            label="Plan name"
            htmlFor="plan-name"
            hint="e.g. Baseline, Retire at 62, Move to Nevada"
          >
            <TextInput
              id="plan-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Baseline"
              maxLength={120}
              required
            />
          </Field>
          <Button type="submit" disabled={saving || name.trim() === ""}>
            {saving ? "Saving…" : "Save as new plan"}
          </Button>
        </form>
      </Card>

      {plans.length === 0 ? (
        <Card>
          <p className="muted center">
            No saved plans yet. Save your current setup above to capture a snapshot you can return
            to later.
          </p>
        </Card>
      ) : (
        <div className="account-list">
          {plans.map((p) => (
            <div className="account-row" key={p.id}>
              <div className="account-main">
                <span className="account-name">{p.name}</span>
                <span className="account-meta muted">{planSummary(p.contents)}</span>
                <span className="account-meta muted">Saved {formatDate(p.created_at)}</span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => handleLoad(p)} disabled={busyId === p.id}>
                  {busyId === p.id ? "Loading…" : "Load"}
                </Button>
                <Button variant="ghost" onClick={() => handleRename(p)}>
                  Rename
                </Button>
                <Button variant="ghost" onClick={() => handleDelete(p)}>
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
