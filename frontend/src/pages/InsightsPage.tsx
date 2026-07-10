import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { Insight } from "../api/types";
import { Alert, Card } from "../components/ui";
import { CATEGORY_LABELS, groupInsightsBySeverity, SEVERITY_LABELS } from "../data/insights";

export function InsightsPage() {
  const [insights, setInsights] = useState<Insight[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        setInsights(await api.getInsights());
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load insights");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  if (loading) return <p className="muted">Loading insights…</p>;

  const groups = groupInsightsBySeverity(insights);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Insights</h1>
          <p className="muted">
            Automatically generated from your plan: ACA/IRMAA/RMD reminders, cash-flow and spending
            anomalies, and portfolio and sequence-of-return risk.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {groups.length === 0 && !error && (
        <Card>
          <p className="muted center">No insights right now — everything looks on track.</p>
        </Card>
      )}

      {groups.map((group) => (
        <Card title={SEVERITY_LABELS[group.severity]} key={group.severity}>
          <div className="account-list">
            {group.items.map((insight, idx) => (
              <div className="account-row" key={`${insight.category}-${idx}`}>
                <div className="account-main">
                  <span className="account-name">{insight.title}</span>
                  <span className="account-meta muted">{CATEGORY_LABELS[insight.category]}</span>
                  <span className="account-meta">{insight.message}</span>
                </div>
              </div>
            ))}
          </div>
        </Card>
      ))}
    </div>
  );
}
