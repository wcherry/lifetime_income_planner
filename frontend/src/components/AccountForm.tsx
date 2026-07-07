import { useState, type FormEvent } from "react";
import type {
  Account,
  AccountCategory,
  AccountRequest,
  AccountType,
} from "../api/types";
import {
  CATEGORY_OPTIONS,
  OWNER_OPTIONS,
  TYPES_BY_CATEGORY,
} from "../data/accounts";
import { Alert, Button, Field, Select, TextInput } from "./ui";

interface Props {
  initial?: Account;
  onSubmit: (payload: AccountRequest) => Promise<void>;
  onCancel: () => void;
}

function toFormState(a?: Account): AccountRequest & {
  useAllocation: boolean;
} {
  return {
    name: a?.name ?? "",
    category: a?.category ?? "taxable",
    account_type: a?.account_type ?? "brokerage",
    owner: a?.owner ?? "self",
    current_balance: a?.current_balance ?? 0,
    expected_roi: a?.expected_roi ?? 6,
    dividend_yield: a?.dividend_yield ?? 0,
    cost_basis: a?.cost_basis ?? null,
    allocation_stock_pct: a?.allocation_stock_pct ?? null,
    allocation_bond_pct: a?.allocation_bond_pct ?? null,
    allocation_cash_pct: a?.allocation_cash_pct ?? null,
    withdrawal_restrictions: a?.withdrawal_restrictions ?? "",
    useAllocation: a?.allocation_stock_pct != null,
  };
}

export function AccountForm({ initial, onSubmit, onCancel }: Props) {
  const [form, setForm] = useState(() => toFormState(initial));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  // When the category changes, snap the type to the first valid type for it.
  function changeCategory(category: AccountCategory) {
    const firstType = TYPES_BY_CATEGORY[category][0].value;
    setForm((f) => ({ ...f, category, account_type: firstType }));
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);

    if (form.useAllocation) {
      const sum =
        (form.allocation_stock_pct ?? 0) +
        (form.allocation_bond_pct ?? 0) +
        (form.allocation_cash_pct ?? 0);
      if (sum !== 100) {
        setError("Allocation percentages must sum to 100.");
        return;
      }
    }

    const payload: AccountRequest = {
      name: form.name,
      category: form.category,
      account_type: form.account_type,
      owner: form.owner,
      current_balance: Number(form.current_balance),
      expected_roi: Number(form.expected_roi),
      dividend_yield: Number(form.dividend_yield),
      cost_basis:
        form.category === "taxable" && form.cost_basis != null
          ? Number(form.cost_basis)
          : null,
      allocation_stock_pct: form.useAllocation ? Number(form.allocation_stock_pct ?? 0) : null,
      allocation_bond_pct: form.useAllocation ? Number(form.allocation_bond_pct ?? 0) : null,
      allocation_cash_pct: form.useAllocation ? Number(form.allocation_cash_pct ?? 0) : null,
      withdrawal_restrictions: form.withdrawal_restrictions?.trim() || null,
    };

    setSaving(true);
    try {
      await onSubmit(payload);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save account");
    } finally {
      setSaving(false);
    }
  }

  const typeOptions = TYPES_BY_CATEGORY[form.category];

  return (
    <form onSubmit={submit} className="account-form">
      {error && <Alert kind="error">{error}</Alert>}

      <Field label="Account name" htmlFor="name">
        <TextInput
          id="name"
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          placeholder="e.g. Fidelity Brokerage"
          required
        />
      </Field>

      <div className="grid-3">
        <Field label="Tax category" htmlFor="category">
          <Select
            id="category"
            value={form.category}
            onChange={(e) => changeCategory(e.target.value as AccountCategory)}
          >
            {CATEGORY_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Account type" htmlFor="type">
          <Select
            id="type"
            value={form.account_type}
            onChange={(e) => set("account_type", e.target.value as AccountType)}
          >
            {typeOptions.map((o) => (
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
            onChange={(e) => set("owner", e.target.value as typeof form.owner)}
          >
            {OWNER_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>

      <div className="grid-3">
        <Field label="Current balance ($)" htmlFor="balance">
          <TextInput
            id="balance"
            type="number"
            min={0}
            step="0.01"
            value={form.current_balance}
            onChange={(e) => set("current_balance", Number(e.target.value))}
            required
          />
        </Field>
        <Field label="Expected ROI (%)" htmlFor="roi" hint="Annual, e.g. 6.5">
          <TextInput
            id="roi"
            type="number"
            step="0.1"
            value={form.expected_roi}
            onChange={(e) => set("expected_roi", Number(e.target.value))}
            required
          />
        </Field>
        <Field label="Dividend yield (%)" htmlFor="dividend">
          <TextInput
            id="dividend"
            type="number"
            step="0.1"
            min={0}
            value={form.dividend_yield}
            onChange={(e) => set("dividend_yield", Number(e.target.value))}
          />
        </Field>
      </div>

      {form.category === "taxable" && (
        <Field
          label="Cost basis ($)"
          htmlFor="cost_basis"
          hint="Used later for capital-gains estimates."
        >
          <TextInput
            id="cost_basis"
            type="number"
            min={0}
            step="0.01"
            value={form.cost_basis ?? ""}
            onChange={(e) =>
              set("cost_basis", e.target.value === "" ? null : Number(e.target.value))
            }
          />
        </Field>
      )}

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={form.useAllocation}
          onChange={(e) => set("useAllocation", e.target.checked)}
        />
        Specify target allocation
      </label>

      {form.useAllocation && (
        <div className="grid-3">
          <Field label="Stock (%)" htmlFor="alloc_stock">
            <TextInput
              id="alloc_stock"
              type="number"
              min={0}
              max={100}
              value={form.allocation_stock_pct ?? 0}
              onChange={(e) => set("allocation_stock_pct", Number(e.target.value))}
            />
          </Field>
          <Field label="Bond (%)" htmlFor="alloc_bond">
            <TextInput
              id="alloc_bond"
              type="number"
              min={0}
              max={100}
              value={form.allocation_bond_pct ?? 0}
              onChange={(e) => set("allocation_bond_pct", Number(e.target.value))}
            />
          </Field>
          <Field label="Cash (%)" htmlFor="alloc_cash">
            <TextInput
              id="alloc_cash"
              type="number"
              min={0}
              max={100}
              value={form.allocation_cash_pct ?? 0}
              onChange={(e) => set("allocation_cash_pct", Number(e.target.value))}
            />
          </Field>
        </div>
      )}

      <Field label="Withdrawal restrictions" htmlFor="restrictions">
        <TextInput
          id="restrictions"
          value={form.withdrawal_restrictions ?? ""}
          onChange={(e) => set("withdrawal_restrictions", e.target.value)}
          placeholder="e.g. Age 59½ or 10% penalty"
        />
      </Field>

      <div className="form-actions">
        <Button type="submit" disabled={saving}>
          {saving ? "Saving…" : initial ? "Save changes" : "Add account"}
        </Button>
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
