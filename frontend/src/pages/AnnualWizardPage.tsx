import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { AnnualWizard } from "../api/types";
import { Alert, Button, Card } from "../components/ui";

/**
 * Annual planning wizard (roadmap Phase 5, feature 8): a guided checklist
 * that sequences through the existing settings pages once a year, rather
 * than duplicating their forms — each step links to the real page and shows
 * a live status hint pulled from the user's actual data.
 */
export function AnnualWizardPage() {
  const [wizard, setWizard] = useState<AnnualWizard | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [completing, setCompleting] = useState(false);

  useEffect(() => {
    load();
  }, []);

  async function load() {
    setLoading(true);
    try {
      setWizard(await api.getAnnualWizard());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load the annual wizard");
    } finally {
      setLoading(false);
    }
  }

  async function handleComplete() {
    setError(null);
    setCompleting(true);
    try {
      await api.completeAnnualWizard();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to mark this year's planning complete");
    } finally {
      setCompleting(false);
    }
  }

  if (loading) return <p className="muted">Loading…</p>;
  if (!wizard) return error ? <Alert kind="error">{error}</Alert> : null;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Annual plan review — {wizard.year}</h1>
          <p className="muted">
            Once a year, work through each step below to keep your plan current. Each step links
            to the page where you make the actual updates.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}
      {wizard.completed && (
        <Alert kind="success">You've marked {wizard.year}'s planning review complete.</Alert>
      )}

      <div className="account-list">
        {wizard.steps.map((step, idx) => (
          <div className="account-row" key={step.key}>
            <div className="account-main">
              <span className="account-name">
                {idx + 1}. {step.label}
              </span>
              <span className="account-meta muted">{step.hint}</span>
            </div>
            <div className="account-actions">
              <Link className="btn btn-ghost" to={step.route}>
                Go
              </Link>
            </div>
          </div>
        ))}
      </div>

      <Card>
        <p className="muted">
          Once you've reviewed everything above, mark this year's planning complete.
        </p>
        <Button onClick={handleComplete} disabled={completing || wizard.completed}>
          {wizard.completed
            ? "Completed"
            : completing
              ? "Saving…"
              : `Mark ${wizard.year}'s planning complete`}
        </Button>
      </Card>
    </div>
  );
}
