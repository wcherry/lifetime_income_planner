import { useState, type FormEvent } from "react";
import type {
  SpendingCategory,
  SpendingFrequency,
  SpendingItem,
  SpendingRequest,
} from "../api/types";
import {
  SPENDING_CATEGORY_OPTIONS,
  SPENDING_FREQUENCY_OPTIONS,
} from "../data/spending";
import { Alert, Button, Field, Select, TextInput } from "./ui";

interface Props {
  initial?: SpendingItem;
  onSubmit: (payload: SpendingRequest) => Promise<void>;
  onCancel: () => void;
}

function toFormState(s?: SpendingItem) {
  return {
    name: s?.name ?? "",
    category: (s?.category ?? "essential") as SpendingCategory,
    amount: s?.amount ?? 0,
    frequency: (s?.frequency ?? "monthly") as SpendingFrequency,
    inflation_adjusted: s?.inflation_adjusted ?? true,
    start_year: s?.start_year ?? null,
    end_year: s?.end_year ?? null,
    notes: s?.notes ?? "",
  };
}

export function SpendingForm({ initial, onSubmit, onCancel }: Props) {
  const [form, setForm] = useState(() => toFormState(initial));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const payload: SpendingRequest = {
      name: form.name,
      category: form.category,
      amount: Number(form.amount),
      frequency: form.frequency,
      inflation_adjusted: form.inflation_adjusted,
      start_year: form.start_year != null ? Number(form.start_year) : null,
      end_year: form.end_year != null ? Number(form.end_year) : null,
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

      <Field label="Description" htmlFor="name">
        <TextInput
          id="name"
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          placeholder="e.g. Groceries"
          required
        />
      </Field>

      <div className="grid-3">
        <Field label="Category" htmlFor="category">
          <Select
            id="category"
            value={form.category}
            onChange={(e) => set("category", e.target.value as SpendingCategory)}
          >
            {SPENDING_CATEGORY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
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
            onChange={(e) => set("frequency", e.target.value as SpendingFrequency)}
          >
            {SPENDING_FREQUENCY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>

      <div className="grid-2">
        <Field label="Start year (optional)" htmlFor="start_year">
          <TextInput
            id="start_year"
            type="number"
            placeholder="e.g. 2032"
            value={form.start_year ?? ""}
            onChange={(e) =>
              set("start_year", e.target.value === "" ? null : Number(e.target.value))
            }
          />
        </Field>
        <Field label="End year (optional)" htmlFor="end_year">
          <TextInput
            id="end_year"
            type="number"
            placeholder="e.g. 2040"
            value={form.end_year ?? ""}
            onChange={(e) =>
              set("end_year", e.target.value === "" ? null : Number(e.target.value))
            }
          />
        </Field>
      </div>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={form.inflation_adjusted}
          onChange={(e) => set("inflation_adjusted", e.target.checked)}
        />
        Adjust this amount for inflation
      </label>

      <Field label="Notes" htmlFor="notes">
        <TextInput
          id="notes"
          value={form.notes ?? ""}
          onChange={(e) => set("notes", e.target.value)}
        />
      </Field>

      <div className="form-actions">
        <Button type="submit" disabled={saving}>
          {saving ? "Saving…" : initial ? "Save changes" : "Add expense"}
        </Button>
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
