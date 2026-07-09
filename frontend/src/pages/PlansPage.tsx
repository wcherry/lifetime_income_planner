import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { Plan, PlanVersion } from "../api/types";
import { Alert, Button, Card, Field, TextInput } from "../components/ui";
import { planSummary } from "../data/plans";

function formatDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

function formatDateTime(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
      });
}

export function PlansPage() {
  const [plans, setPlans] = useState<Plan[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [historyOpenId, setHistoryOpenId] = useState<string | null>(null);
  const [versionsByPlan, setVersionsByPlan] = useState<Record<string, PlanVersion[]>>({});
  const [versionsLoading, setVersionsLoading] = useState<string | null>(null);

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

  async function handleClone(plan: Plan) {
    const suggested = `${plan.name} copy`;
    const next = window.prompt("Name for the cloned scenario", suggested);
    if (next == null) return;
    const trimmed = next.trim();
    setError(null);
    setNotice(null);
    setBusyId(plan.id);
    try {
      const clone = await api.clonePlan(plan.id, trimmed === "" ? {} : { name: trimmed });
      setNotice(`Cloned "${plan.name}" as "${clone.name}".`);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clone plan");
    } finally {
      setBusyId(null);
    }
  }

  async function loadVersions(planId: string) {
    setVersionsLoading(planId);
    try {
      const versions = await api.listPlanVersions(planId);
      setVersionsByPlan((cur) => ({ ...cur, [planId]: versions }));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load version history");
    } finally {
      setVersionsLoading(null);
    }
  }

  function toggleHistory(plan: Plan) {
    if (historyOpenId === plan.id) {
      setHistoryOpenId(null);
      return;
    }
    setHistoryOpenId(plan.id);
    loadVersions(plan.id);
  }

  async function handleUpdateSnapshot(plan: Plan) {
    if (
      !window.confirm(
        `Refresh "${plan.name}" with your current working data? The scenario's previous data ` +
          `is kept in its version history, not lost.`,
      )
    )
      return;
    setError(null);
    setNotice(null);
    setBusyId(plan.id);
    try {
      await api.updatePlanSnapshot(plan.id);
      setNotice(`Updated "${plan.name}" with your current data.`);
      await refresh();
      if (historyOpenId === plan.id) await loadVersions(plan.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update plan");
    } finally {
      setBusyId(null);
    }
  }

  async function handleRestore(plan: Plan, version: PlanVersion) {
    if (
      !window.confirm(
        `Restore "${plan.name}" to its ${formatDateTime(version.created_at)} version? ` +
          `The scenario's current data is kept in its version history, not lost.`,
      )
    )
      return;
    setError(null);
    setNotice(null);
    setBusyId(plan.id);
    try {
      await api.restorePlanVersion(plan.id, version.id);
      setNotice(`Restored "${plan.name}" to its ${formatDateTime(version.created_at)} version.`);
      await refresh();
      await loadVersions(plan.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to restore version");
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
            current working data. Clone a scenario to branch off it and iterate without touching
            the original.
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
          {plans.map((p) => {
            const parentName = p.parent_plan_id
              ? (plans.find((other) => other.id === p.parent_plan_id)?.name ?? "a deleted scenario")
              : null;
            return (
              <div className="account-row" key={p.id}>
                <div className="account-main">
                  <span className="account-name">{p.name}</span>
                  <span className="account-meta muted">{planSummary(p.contents)}</span>
                  <span className="account-meta muted">Saved {formatDate(p.created_at)}</span>
                  {parentName && (
                    <span className="account-meta muted">Cloned from {parentName}</span>
                  )}
                </div>
                <div className="account-actions">
                  <Button variant="ghost" onClick={() => handleLoad(p)} disabled={busyId === p.id}>
                    {busyId === p.id ? "Loading…" : "Load"}
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => handleUpdateSnapshot(p)}
                    disabled={busyId === p.id}
                  >
                    Update
                  </Button>
                  <Button variant="ghost" onClick={() => handleClone(p)} disabled={busyId === p.id}>
                    Clone
                  </Button>
                  <Button variant="ghost" onClick={() => toggleHistory(p)}>
                    {historyOpenId === p.id ? "Hide history" : "History"}
                  </Button>
                  <Button variant="ghost" onClick={() => handleRename(p)}>
                    Rename
                  </Button>
                  <Button variant="ghost" onClick={() => handleDelete(p)}>
                    Delete
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {historyOpenId && (
        <PlanHistoryCard
          plan={plans.find((p) => p.id === historyOpenId) ?? null}
          versions={versionsByPlan[historyOpenId] ?? []}
          loading={versionsLoading === historyOpenId}
          busy={busyId === historyOpenId}
          onRestore={handleRestore}
        />
      )}
    </div>
  );
}

/**
 * Historical scenario snapshots (roadmap Phase 4, feature 7): the timeline of
 * past versions for one plan, each restorable back to current.
 */
function PlanHistoryCard({
  plan,
  versions,
  loading,
  busy,
  onRestore,
}: {
  plan: Plan | null;
  versions: PlanVersion[];
  loading: boolean;
  busy: boolean;
  onRestore: (plan: Plan, version: PlanVersion) => void;
}) {
  if (!plan) return null;
  return (
    <Card title={`History · ${plan.name}`}>
      <p className="muted">
        Every time this scenario is updated with fresh data, its previous version is kept here.
        Restoring brings a past version back as the current one — nothing is ever deleted.
      </p>
      {loading ? (
        <p className="muted center">Loading history…</p>
      ) : versions.length === 0 ? (
        <p className="muted center">
          No history yet — updating this scenario with current data will start its timeline.
        </p>
      ) : (
        <div className="account-list">
          {versions.map((v) => (
            <div className="account-row" key={v.id}>
              <div className="account-main">
                <span className="account-name">{formatDateTime(v.created_at)}</span>
                <span className="account-meta muted">{planSummary(v.contents)}</span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => onRestore(plan, v)} disabled={busy}>
                  Restore
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
