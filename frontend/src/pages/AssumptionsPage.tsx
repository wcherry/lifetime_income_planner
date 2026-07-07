import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { AssumptionsRequest } from "../api/types";
import { Alert, Button, Card, Field, TextInput } from "../components/ui";

// Matches the backend defaults in models/assumptions.rs.
const DEFAULTS: AssumptionsRequest = {
  inflation_rate: 2.5,
  investment_return_rate: 6.0,
  healthcare_inflation_rate: 4.5,
  social_security_cola_rate: 2.0,
};

export function AssumptionsPage() {
  const [form, setForm] = useState<AssumptionsRequest>(DEFAULTS);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [usingDefaults, setUsingDefaults] = useState(true);

  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const a = await api.getAssumptions();
        if (active) {
          setForm({
            inflation_rate: a.inflation_rate,
            investment_return_rate: a.investment_return_rate,
            healthcare_inflation_rate: a.healthcare_inflation_rate,
            social_security_cola_rate: a.social_security_cola_rate,
          });
          setUsingDefaults(a.is_default);
        }
      } catch (err) {
        if (active)
          setError(err instanceof Error ? err.message : "Failed to load assumptions");
      } finally {
        if (active) setLoading(false);
      }
    }
    load();
    return () => {
      active = false;
    };
  }, []);

  function update<K extends keyof AssumptionsRequest>(
    key: K,
    value: AssumptionsRequest[K],
  ) {
    setForm((f) => ({ ...f, [key]: value }));
    setSaved(false);
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(false);
    setSaving(true);
    try {
      const payload: AssumptionsRequest = {
        inflation_rate: Number(form.inflation_rate),
        investment_return_rate: Number(form.investment_return_rate),
        healthcare_inflation_rate: Number(form.healthcare_inflation_rate),
        social_security_cola_rate: Number(form.social_security_cola_rate),
      };
      await api.saveAssumptions(payload);
      setSaved(true);
      setUsingDefaults(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save assumptions");
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <p className="muted">Loading assumptions…</p>;

  return (
    <Card title="Inflation & ROI assumptions">
      <p className="muted">
        These rates drive every projection. We start you with reasonable
        defaults — adjust them to match your own outlook.
      </p>
      <form onSubmit={onSubmit}>
        {error && <Alert kind="error">{error}</Alert>}
        {saved && <Alert kind="success">Assumptions saved.</Alert>}
        {usingDefaults && !saved && (
          <Alert kind="success">Showing default assumptions. Save to make them yours.</Alert>
        )}

        <div className="grid-2">
          <Field
            label="General inflation (%)"
            htmlFor="inflation"
            hint="Annual rise in everyday prices."
          >
            <TextInput
              id="inflation"
              type="number"
              step="0.1"
              value={form.inflation_rate}
              onChange={(e) => update("inflation_rate", Number(e.target.value))}
              required
            />
          </Field>
          <Field
            label="Investment return (%)"
            htmlFor="roi"
            hint="Default expected annual portfolio return."
          >
            <TextInput
              id="roi"
              type="number"
              step="0.1"
              value={form.investment_return_rate}
              onChange={(e) =>
                update("investment_return_rate", Number(e.target.value))
              }
              required
            />
          </Field>
        </div>

        <div className="grid-2">
          <Field
            label="Healthcare inflation (%)"
            htmlFor="healthcare"
            hint="Healthcare costs typically outpace general inflation."
          >
            <TextInput
              id="healthcare"
              type="number"
              step="0.1"
              value={form.healthcare_inflation_rate}
              onChange={(e) =>
                update("healthcare_inflation_rate", Number(e.target.value))
              }
              required
            />
          </Field>
          <Field
            label="Social Security COLA (%)"
            htmlFor="cola"
            hint="Assumed annual cost-of-living adjustment."
          >
            <TextInput
              id="cola"
              type="number"
              step="0.1"
              value={form.social_security_cola_rate}
              onChange={(e) =>
                update("social_security_cola_rate", Number(e.target.value))
              }
              required
            />
          </Field>
        </div>

        <Button type="submit" disabled={saving}>
          {saving ? "Saving…" : "Save assumptions"}
        </Button>
      </form>
    </Card>
  );
}
