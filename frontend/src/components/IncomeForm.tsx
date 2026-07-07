import { useState, type FormEvent } from "react";
import type {
  IncomeFrequency,
  IncomeOwner,
  IncomeRequest,
  IncomeSource,
  IncomeType,
  Taxability,
} from "../api/types";
import {
  INCOME_FREQUENCY_OPTIONS,
  INCOME_OWNER_OPTIONS,
  INCOME_TYPE_OPTIONS,
  TAXABILITY_OPTIONS,
} from "../data/income";
import { Alert, Button, Field, Select, TextInput } from "./ui";

interface Props {
  initial?: IncomeSource;
  onSubmit: (payload: IncomeRequest) => Promise<void>;
  onCancel: () => void;
}

function toFormState(i?: IncomeSource) {
  return {
    name: i?.name ?? "",
    income_type: (i?.income_type ?? "social_security") as IncomeType,
    owner: (i?.owner ?? "self") as IncomeOwner,
    amount: i?.amount ?? 0,
    frequency: (i?.frequency ?? "monthly") as IncomeFrequency,
    start_date: i?.start_date ?? "",
    end_date: i?.end_date ?? "",
    growth_rate: i?.growth_rate ?? 0,
    cola: i?.cola ?? false,
    taxability: (i?.taxability ?? "taxable") as Taxability,
    notes: i?.notes ?? "",
  };
}

export function IncomeForm({ initial, onSubmit, onCancel }: Props) {
  const [form, setForm] = useState(() => toFormState(initial));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const payload: IncomeRequest = {
      name: form.name,
      income_type: form.income_type,
      owner: form.owner,
      amount: Number(form.amount),
      frequency: form.frequency,
      start_date: form.start_date,
      end_date: form.end_date || null,
      growth_rate: Number(form.growth_rate),
      cola: form.cola,
      taxability: form.taxability,
      notes: form.notes?.trim() || null,
    };
    setSaving(true);
    try {
      await onSubmit(payload);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  }

  return (
    <form onSubmit={submit} className="account-form">
      {error && <Alert kind="error">{error}</Alert>}

      <Field label="Name" htmlFor="name">
        <TextInput
          id="name"
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          placeholder="e.g. Social Security"
          required
        />
      </Field>

      <div className="grid-3">
        <Field label="Type" htmlFor="type">
          <Select
            id="type"
            value={form.income_type}
            onChange={(e) => set("income_type", e.target.value as IncomeType)}
          >
            {INCOME_TYPE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Owner" htmlFor="owner">
          <Select
            id="owner"
            value={form.owner}
            onChange={(e) => set("owner", e.target.value as IncomeOwner)}
          >
            {INCOME_OWNER_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Taxability" htmlFor="taxability">
          <Select
            id="taxability"
            value={form.taxability}
            onChange={(e) => set("taxability", e.target.value as Taxability)}
          >
            {TAXABILITY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>

      <div className="grid-2">
        <Field label="Amount ($)" htmlFor="amount">
          <TextInput
            id="amount"
            type="number"
            min={0}
            step="0.01"
            value={form.amount}
            onChange={(e) => set("amount", Number(e.target.value))}
            required
          />
        </Field>
        <Field label="Frequency" htmlFor="frequency">
          <Select
            id="frequency"
            value={form.frequency}
            onChange={(e) => set("frequency", e.target.value as IncomeFrequency)}
          >
            {INCOME_FREQUENCY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>

      <div className="grid-2">
        <Field label="Start date" htmlFor="start_date">
          <TextInput
            id="start_date"
            type="date"
            value={form.start_date}
            onChange={(e) => set("start_date", e.target.value)}
            required
          />
        </Field>
        <Field label="End date (optional)" htmlFor="end_date" hint="Blank = for life.">
          <TextInput
            id="end_date"
            type="date"
            value={form.end_date}
            onChange={(e) => set("end_date", e.target.value)}
          />
        </Field>
      </div>

      <div className="grid-2">
        <Field label="Annual growth (%)" htmlFor="growth" hint="e.g. raises">
          <TextInput
            id="growth"
            type="number"
            step="0.1"
            value={form.growth_rate}
            onChange={(e) => set("growth_rate", Number(e.target.value))}
          />
        </Field>
        <Field label="Notes" htmlFor="notes">
          <TextInput
            id="notes"
            value={form.notes ?? ""}
            onChange={(e) => set("notes", e.target.value)}
          />
        </Field>
      </div>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={form.cola}
          onChange={(e) => set("cola", e.target.checked)}
        />
        Receives an annual cost-of-living adjustment (COLA)
      </label>

      <div className="form-actions">
        <Button type="submit" disabled={saving}>
          {saving ? "Saving…" : initial ? "Save changes" : "Add income"}
        </Button>
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
