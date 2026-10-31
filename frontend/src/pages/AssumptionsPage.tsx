import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type {
  AssumptionsRequest,
  RothConversionMode,
  WithdrawalStrategy,
} from "../api/types";
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
  medicare_part_b_annual_premium: 2_220,
  avoid_early_withdrawal_penalty: false,
  medicare_enrollment_age: 65,
  roth_conversion_mode: "fixed_ceiling",
};

const ROTH_CONVERSION_MODE_OPTIONS: { value: RothConversionMode; label: string }[] = [
  { value: "fixed_ceiling", label: "Fixed ceiling (convert to a flat taxable-income target)" },
  {
    value: "irmaa_aware",
    label: "IRMAA & RMD aware (target the next Medicare IRMAA tier, tapering as RMDs grow)",
  },
  {
    value: "aca_aware",
    label: "ACA aware (stay under the ACA subsidy cutoff; flat ceiling in other years)",
  },
];

const WITHDRAWAL_STRATEGY_OPTIONS: { value: WithdrawalStrategy; label: string }[] = [
  { value: "conventional", label: "Conventional (taxable → tax-deferred → tax-free)" },
  { value: "tax_optimized", label: "Tax-optimized (minimize tax at the margin each year)" },
  {
    value: "maximize_income",
    label: "Maximize income (simulate bracket-filling withdrawals to reduce future RMD tax)",
  },
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
            medicare_part_b_annual_premium: a.medicare_part_b_annual_premium,
            avoid_early_withdrawal_penalty: a.avoid_early_withdrawal_penalty,
            medicare_enrollment_age: a.medicare_enrollment_age,
            roth_conversion_mode: a.roth_conversion_mode,
          });
          setUsingDefaults(a.is_default);
        }
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Failed to load assumptions");
      } finally {
        if (active) setLoading(false);
      }
    }
    load();
    return () => {
      active = false;
    };
  }, []);

  function update<K extends keyof AssumptionsRequest>(key: K, value: AssumptionsRequest[K]) {
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
        medicare_part_b_annual_premium: Number(form.medicare_part_b_annual_premium),
        avoid_early_withdrawal_penalty: form.avoid_early_withdrawal_penalty,
        medicare_enrollment_age: Number(form.medicare_enrollment_age),
        roth_conversion_mode: form.roth_conversion_mode,
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
        These rates drive every projection. We start you with reasonable defaults — adjust them to
        match your own outlook.
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
              onChange={(e) => update("investment_return_rate", Number(e.target.value))}
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
              onChange={(e) => update("healthcare_inflation_rate", Number(e.target.value))}
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
              onChange={(e) => update("social_security_cola_rate", Number(e.target.value))}
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
          <Field label="Sizing mode" htmlFor="roth-mode">
            <Select
              id="roth-mode"
              value={form.roth_conversion_mode}
              onChange={(e) => update("roth_conversion_mode", e.target.value as RothConversionMode)}
            >
              {ROTH_CONVERSION_MODE_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <div className="grid-2">
          <Field
            label="Convert up to taxable income ($)"
            htmlFor="roth-ceiling"
            hint={
              form.roth_conversion_mode === "fixed_ceiling"
                ? "Fill each year's taxable income to this level. 0 disables conversions."
                : form.roth_conversion_mode === "irmaa_aware"
                  ? "Optional extra cap on top of the IRMAA target. 0 means no extra cap."
                  : "Optional extra cap on top of the ACA cutoff. In years the cutoff doesn't apply, this is the flat ceiling; 0 means no conversion then."
            }
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
          thumb. <strong>Tax-optimized</strong> also realizes the lowest-gain taxable lots first,
          and in years where realizing a taxable gain would cost more than an equivalent ordinary
          withdrawal, draws from tax-deferred accounts first instead.{" "}
          <strong>Maximize income</strong> builds on tax-optimized: it simulates several levels of
          voluntary pre-RMD tax-deferred withdrawals that fill a low bracket today, and keeps
          whichever level leaves the most after-tax lifetime income once RMDs are eventually forced.
        </p>

        <div className="grid-2">
          <Field label="Strategy" htmlFor="withdrawal-strategy">
            <Select
              id="withdrawal-strategy"
              value={form.withdrawal_strategy}
              onChange={(e) => update("withdrawal_strategy", e.target.value as WithdrawalStrategy)}
            >
              {WITHDRAWAL_STRATEGY_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={form.avoid_early_withdrawal_penalty}
            onChange={(e) => update("avoid_early_withdrawal_penalty", e.target.checked)}
          />
          Don't incur penalties
        </label>
        <p className="muted">
          Traditional IRA and 401(k) withdrawals taken before age 59½ owe a 10% early-withdrawal
          penalty. When checked, the planner never draws from those accounts before then — even if
          spending goes unfunded as a result — instead of paying the penalty. Roth conversions are
          unaffected either way.
        </p>

        <h3 className="assumptions-subhead">ACA health insurance subsidy</h3>
        <p className="muted">
          Before Medicare (age 65), a marketplace plan may qualify for a premium tax credit that
          caps what you pay based on your income. Enter your <strong>benchmark premium</strong> —
          the annual cost of the second-lowest-cost silver plan for your household (from
          HealthCare.gov or your state exchange) — and the planner estimates the subsidy each
          pre-Medicare year, net of the income your withdrawals and Roth conversions create. Set it
          to $0 to skip ACA modeling. It grows with your healthcare inflation rate.
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

        <h3 className="assumptions-subhead">Medicare Part B</h3>
        <p className="muted">
          From age 65, Medicare Part B has a standard monthly premium — higher earners pay more via
          an income-based IRMAA surcharge, modeled in a later phase. The planner adds the standard
          premium to your annual costs automatically once each person in the household turns 65. Set
          it to $0 if you expect other coverage (e.g. a spouse's employer plan) to pay it instead.
        </p>

        <div className="grid-2">
          <Field
            label="Part B premium ($/yr)"
            htmlFor="medicare-part-b"
            hint="2025 standard premium is $185.00/mo ($2,220/yr). Charged per person, per year, from age 65."
          >
            <TextInput
              id="medicare-part-b"
              type="number"
              step="100"
              min="0"
              value={form.medicare_part_b_annual_premium}
              onChange={(e) => update("medicare_part_b_annual_premium", Number(e.target.value))}
              required
            />
          </Field>
        </div>

        <div className="grid-2">
          <Field
            label="Medicare enrollment age"
            htmlFor="medicare-enrollment-age"
            hint="65 unless you delay enrollment (e.g. other creditable coverage). Between 65 and 70."
          >
            <TextInput
              id="medicare-enrollment-age"
              type="number"
              step="1"
              min="65"
              max="70"
              value={form.medicare_enrollment_age}
              onChange={(e) => update("medicare_enrollment_age", Number(e.target.value))}
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
