import { useState, type FormEvent } from "react";
import type {
  CashFlowDirection,
  EventRecurrence,
  LifeEvent,
  LifeEventRequest,
  LifeEventType,
} from "../api/types";
import {
  DIRECTION_OPTIONS,
  LIFE_EVENT_TYPE_OPTIONS,
  RECURRENCE_OPTIONS,
} from "../data/lifeEvents";
import { Alert, Button, Field, Select, TextInput } from "./ui";

interface Props {
  initial?: LifeEvent;
  onSubmit: (payload: LifeEventRequest) => Promise<void>;
  onCancel: () => void;
}

function toFormState(e?: LifeEvent) {
  return {
    name: e?.name ?? "",
    event_type: (e?.event_type ?? "sell_house") as LifeEventType,
    event_date: e?.event_date ?? "",
    direction: (e?.direction ?? "inflow") as CashFlowDirection,
    amount: e?.amount ?? 0,
    taxable: e?.taxable ?? false,
    inflation_adjusted: e?.inflation_adjusted ?? false,
    recurrence: (e?.recurrence ?? "one_time") as EventRecurrence,
    end_date: e?.end_date ?? "",
    notes: e?.notes ?? "",
  };
}

export function LifeEventForm({ initial, onSubmit, onCancel }: Props) {
  const [form, setForm] = useState(() => toFormState(initial));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  const recurring = form.recurrence !== "one_time";

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const payload: LifeEventRequest = {
      name: form.name,
      event_type: form.event_type,
      event_date: form.event_date,
      direction: form.direction,
      amount: Number(form.amount),
      taxable: form.taxable,
      inflation_adjusted: form.inflation_adjusted,
      recurrence: form.recurrence,
      end_date: recurring ? form.end_date || null : null,
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
          placeholder="e.g. Sell the lake house"
          required
        />
      </Field>

      <div className="grid-2">
        <Field label="Event type" htmlFor="event_type">
          <Select
            id="event_type"
            value={form.event_type}
            onChange={(e) => set("event_type", e.target.value as LifeEventType)}
          >
            {LIFE_EVENT_TYPE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Event date" htmlFor="event_date">
          <TextInput
            id="event_date"
            type="date"
            value={form.event_date}
            onChange={(e) => set("event_date", e.target.value)}
            required
          />
        </Field>
      </div>

      <div className="grid-2">
        <Field label="Direction" htmlFor="direction">
          <Select
            id="direction"
            value={form.direction}
            onChange={(e) => set("direction", e.target.value as CashFlowDirection)}
          >
            {DIRECTION_OPTIONS.map((o) => (
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
      </div>

      <div className="grid-2">
        <Field label="Repeat" htmlFor="recurrence">
          <Select
            id="recurrence"
            value={form.recurrence}
            onChange={(e) => set("recurrence", e.target.value as EventRecurrence)}
          >
            {RECURRENCE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        {recurring && (
          <Field
            label="Repeat until (optional)"
            htmlFor="end_date"
            hint="Blank = repeats indefinitely."
          >
            <TextInput
              id="end_date"
              type="date"
              value={form.end_date}
              onChange={(e) => set("end_date", e.target.value)}
            />
          </Field>
        )}
      </div>

      <Field label="Notes" htmlFor="notes">
        <TextInput
          id="notes"
          value={form.notes ?? ""}
          onChange={(e) => set("notes", e.target.value)}
        />
      </Field>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={form.taxable}
          onChange={(e) => set("taxable", e.target.checked)}
        />
        This cash flow is taxable
      </label>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={form.inflation_adjusted}
          onChange={(e) => set("inflation_adjusted", e.target.checked)}
        />
        Adjust the amount for inflation up to the event date
      </label>

      <div className="form-actions">
        <Button type="submit" disabled={saving}>
          {saving ? "Saving…" : initial ? "Save changes" : "Add event"}
        </Button>
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
