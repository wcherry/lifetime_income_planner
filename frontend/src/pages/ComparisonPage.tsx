import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { Plan, ScenarioComparison } from "../api/types";
import { Alert, Button, Card } from "../components/ui";
import { formatCurrency } from "../data/format";

const MIN_SELECTION = 2;
const MAX_SELECTION = 10;

/**
 * Side-by-side scenario comparison (roadmap Phase 4, feature 2): runs two or
 * more saved plans through the projection engine off their saved snapshots —
 * without touching the live working set — and lines up their headline
 * figures for a direct comparison, the way the roadmap's Comparison Dashboard
 * describes (Taxes · Estate · ACA subsidies · RMD · Net spending · Age money
 * depleted).
 */
export function ComparisonPage() {
  const [plans, setPlans] = useState<Plan[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [results, setResults] = useState<ScenarioComparison[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [comparing, setComparing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        setPlans(await api.listPlans());
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load saved plans");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  function toggle(id: string) {
    setSelected((cur) => {
      if (cur.includes(id)) return cur.filter((x) => x !== id);
      if (cur.length >= MAX_SELECTION) return cur;
      return [...cur, id];
    });
  }

  async function handleCompare() {
    setError(null);
    setComparing(true);
    try {
      const orderedIds = plans.filter((p) => selected.includes(p.id)).map((p) => p.id);
      setResults(await api.compareScenarios({ plan_ids: orderedIds }));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to compare scenarios");
    } finally {
      setComparing(false);
    }
  }

  if (loading) return <p className="muted">Loading saved plans…</p>;

  const canCompare = selected.length >= MIN_SELECTION && selected.length <= MAX_SELECTION;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Compare scenarios</h1>
          <p className="muted">
            Pick {MIN_SELECTION}–{MAX_SELECTION} saved plans to see how they stack up — taxes,
            estate, ACA subsidies, RMDs, net spending, and the age money runs out, if it does.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {plans.length < MIN_SELECTION ? (
        <Card>
          <p className="muted center">
            Save at least {MIN_SELECTION} plans on the{" "}
            <Link to="/plans">Saved plans</Link> page first — comparison runs against saved
            snapshots, not your current working data.
          </p>
        </Card>
      ) : (
        <Card title="Choose scenarios">
          <div className="account-list">
            {plans.map((p) => (
              <label className="account-row compare-row" key={p.id}>
                <input
                  type="checkbox"
                  checked={selected.includes(p.id)}
                  onChange={() => toggle(p.id)}
                  disabled={!selected.includes(p.id) && selected.length >= MAX_SELECTION}
                />
                <span className="account-main">
                  <span className="account-name">{p.name}</span>
                </span>
              </label>
            ))}
          </div>
          <div className="form-actions">
            <Button onClick={handleCompare} disabled={!canCompare || comparing}>
              {comparing ? "Comparing…" : `Compare ${selected.length} scenario${selected.length === 1 ? "" : "s"}`}
            </Button>
          </div>
        </Card>
      )}

      {results && results.length > 0 && (
        <Card title="Comparison">
          <div className="table-scroll">
            <table className="proj-table compare-table">
              <thead>
                <tr>
                  <th>Metric</th>
                  {results.map((r) => (
                    <th key={r.plan_id}>{r.plan_name}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                <CompareRow label="Net worth today" results={results} pick={(r) => r.summary.current_net_worth} />
                <CompareRow
                  label="Estate (ending balance)"
                  results={results}
                  pick={(r) => r.summary.projected_ending_balance}
                />
                <CompareRow
                  label="Lifetime taxes"
                  results={results}
                  pick={(r) => r.summary.total_lifetime_taxes}
                />
                <CompareRow
                  label="Lifetime ACA subsidies"
                  results={results}
                  pick={(r) => r.summary.total_lifetime_aca_subsidies}
                />
                <CompareRow label="Lifetime RMD" results={results} pick={(r) => r.summary.total_lifetime_rmd} />
                <CompareRow
                  label="Lifetime spending"
                  results={results}
                  pick={(r) => r.summary.total_lifetime_spending}
                />
                <tr>
                  <td>Age money depleted</td>
                  {results.map((r) => (
                    <td key={r.plan_id} className={r.depletion_age != null ? "row-warn-cell" : ""}>
                      {r.depletion_age != null ? r.depletion_age : "Never"}
                    </td>
                  ))}
                </tr>
              </tbody>
            </table>
          </div>
        </Card>
      )}
    </div>
  );
}

function CompareRow({
  label,
  results,
  pick,
}: {
  label: string;
  results: ScenarioComparison[];
  pick: (r: ScenarioComparison) => number;
}) {
  return (
    <tr>
      <td>{label}</td>
      {results.map((r) => (
        <td key={r.plan_id} className="num">
          {formatCurrency(pick(r))}
        </td>
      ))}
    </tr>
  );
}
