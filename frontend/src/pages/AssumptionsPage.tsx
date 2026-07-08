import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { AssumptionsRequest, WithdrawalStrategy } from "../api/types";
import { Alert, Button, Card, Field, Select, TextInput } from "../components/ui";

// Matches the backend defaults in models/assumptions.rs.
const DEFAULTS: AssumptionsRequest = {
  inflation_rate: 2.5,
  investment_return_rate: 6.0,
  healthcare_inflation_rate: 4.5,
  social_security_cola_rate: 2.0,
  roth_conversion_ceiling: 0,
  roth_conversion_start_year: null,
  roth_conversion_end_year: null,
  withdrawal_strategy: "conventional",
  aca_benchmark_annual_premium: 0,
};

const WITHDRAWAL_STRATEGY_OPTIONS: { value: WithdrawalStrategy; label: string }[] = [
  { value: "conventional", label: "Conventional (taxable → tax-deferred → tax-free)" },
  { value: "tax_optimized", label: "Tax-optimized (minimize tax at the margin each year)" },
];

/** Parse an optional year input; blank -> null. */
function parseYear(value: string): number | null {
  const n = Number(value);
  return value.trim() === "" || Number.isNaN(n) ? null : n;
}

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
            roth_conversion_ceiling: a.roth_conversion_ceiling,
            roth_conversion_start_year: a.roth_conversion_start_year,
            roth_conversion_end_year: a.roth_conversion_end_year,
            withdrawal_strategy: a.withdrawal_strategy,
            aca_benchmark_annual_premium: a.aca_benchmark_annual_premium,
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
        roth_conversion_ceiling: Number(form.roth_conversion_ceiling),
        roth_conversion_start_year: form.roth_conversion_start_year,
        roth_conversion_end_year: form.roth_conversion_end_year,
        withdrawal_strategy: form.withdrawal_strategy,
        aca_benchmark_annual_premium: Number(form.aca_benchmark_annual_premium),
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

        <h3 className="assumptions-subhead">Roth conversion strategy</h3>
        <p className="muted">
          Optionally convert traditional (tax-deferred) savings to Roth each year up to a target
          taxable income — filling low-income years before RMDs and Social Security push you into
          higher brackets. Set the ceiling to $0 to turn conversions off. Converted dollars move
          into your first Roth account; the tax is funded like any other cash need.
        </p>

        <div className="grid-2">
          <Field
            label="Convert up to taxable income ($)"
            htmlFor="roth-ceiling"
            hint="Fill each year's taxable income to this level. 0 disables conversions."
          >
            <TextInput
              id="roth-ceiling"
              type="number"
              step="1000"
              min="0"
              value={form.roth_conversion_ceiling}
              onChange={(e) => update("roth_conversion_ceiling", Number(e.target.value))}
              required
            />
          </Field>
        </div>

        <div className="grid-2">
          <Field
            label="Start year (optional)"
            htmlFor="roth-start"
            hint="Leave blank to begin at the start of the plan."
          >
            <TextInput
              id="roth-start"
              type="number"
              step="1"
              placeholder="e.g. 2027"
              value={form.roth_conversion_start_year ?? ""}
              onChange={(e) => update("roth_conversion_start_year", parseYear(e.target.value))}
            />
          </Field>
          <Field
            label="End year (optional)"
            htmlFor="roth-end"
            hint="Leave blank to run through the end of the plan."
          >
            <TextInput
              id="roth-end"
              type="number"
              step="1"
              placeholder="e.g. 2034"
              value={form.roth_conversion_end_year ?? ""}
              onChange={(e) => update("roth_conversion_end_year", parseYear(e.target.value))}
            />
          </Field>
        </div>

        <h3 className="assumptions-subhead">Withdrawal sequencing</h3>
        <p className="muted">
          Which order to draw from your accounts each year. <strong>Conventional</strong> drains
          taxable accounts fully before touching tax-deferred, then tax-free — the standard rule of
          thumb. <strong>Tax-optimized</strong> also realizes the lowest-gain taxable lots first, and
          in years where realizing a taxable gain would cost more than an equivalent ordinary
          withdrawal, draws from tax-deferred accounts first instead.
        </p>

        <div className="grid-2">
          <Field label="Strategy" htmlFor="withdrawal-strategy">
            <Select
              id="withdrawal-strategy"
              value={form.withdrawal_strategy}
              onChange={(e) =>
                update("withdrawal_strategy", e.target.value as WithdrawalStrategy)
              }
            >
              {WITHDRAWAL_STRATEGY_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <h3 className="assumptions-subhead">ACA health insurance subsidy</h3>
        <p className="muted">
          Before Medicare (age 65), a marketplace plan may qualify for a premium tax credit that
          caps what you pay based on your income. Enter your <strong>benchmark premium</strong> — the
          annual cost of the second-lowest-cost silver plan for your household (from HealthCare.gov
          or your state exchange) — and the planner estimates the subsidy each pre-Medicare year, net
          of the income your withdrawals and Roth conversions create. Set it to $0 to skip ACA
          modeling. It grows with your healthcare inflation rate.
        </p>

        <div className="grid-2">
          <Field
            label="Benchmark silver premium ($/yr)"
            htmlFor="aca-benchmark"
            hint="Annual second-lowest silver (SLCSP) premium for your household. 0 disables."
          >
            <TextInput
              id="aca-benchmark"
              type="number"
              step="500"
              min="0"
              value={form.aca_benchmark_annual_premium}
              onChange={(e) => update("aca_benchmark_annual_premium", Number(e.target.value))}
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
