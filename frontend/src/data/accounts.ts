import type { AccountCategory, AccountOwner, AccountType } from "../api/types";

export const CATEGORY_OPTIONS: { value: AccountCategory; label: string }[] = [
  { value: "taxable", label: "Taxable" },
  { value: "tax_deferred", label: "Tax-deferred" },
  { value: "tax_free", label: "Tax-free" },
  { value: "other", label: "Other" },
];

export const OWNER_OPTIONS: { value: AccountOwner; label: string }[] = [
  { value: "self", label: "Self" },
  { value: "spouse", label: "Spouse" },
  { value: "joint", label: "Joint" },
];

// Account types grouped by their natural tax category, used to build the
// type dropdown and to suggest a sensible category when a type is picked.
export const TYPES_BY_CATEGORY: Record<
  AccountCategory,
  { value: AccountType; label: string }[]
> = {
  taxable: [
    { value: "brokerage", label: "Brokerage" },
    { value: "savings", label: "Savings" },
    { value: "checking", label: "Checking" },
    { value: "money_market", label: "Money market" },
    { value: "cd", label: "CD" },
  ],
  tax_deferred: [
    { value: "ira", label: "Traditional IRA" },
    { value: "401k", label: "401(k)" },
    { value: "403b", label: "403(b)" },
    { value: "457", label: "457" },
    { value: "sep_ira", label: "SEP IRA" },
  ],
  tax_free: [
    { value: "roth_ira", label: "Roth IRA" },
    { value: "roth_401k", label: "Roth 401(k)" },
    { value: "hsa", label: "HSA" },
  ],
  other: [
    { value: "pension", label: "Pension" },
    { value: "cash_value_life_insurance", label: "Cash value life insurance" },
  ],
};

const ALL_TYPES = Object.values(TYPES_BY_CATEGORY).flat();

export function accountTypeLabel(type: AccountType): string {
  return ALL_TYPES.find((t) => t.value === type)?.label ?? type;
}

export function categoryLabel(category: AccountCategory): string {
  return CATEGORY_OPTIONS.find((c) => c.value === category)?.label ?? category;
}

export function ownerLabel(owner: AccountOwner): string {
  return OWNER_OPTIONS.find((o) => o.value === owner)?.label ?? owner;
}

export { formatCurrency } from "./format";
