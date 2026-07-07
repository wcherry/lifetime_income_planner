import type {
  IncomeFrequency,
  IncomeOwner,
  IncomeType,
  Taxability,
} from "../api/types";

export const INCOME_TYPE_OPTIONS: { value: IncomeType; label: string }[] = [
  { value: "social_security", label: "Social Security" },
  { value: "pension", label: "Pension" },
  { value: "rental", label: "Rental income" },
  { value: "royalties", label: "Royalties" },
  { value: "annuity", label: "Annuity" },
  { value: "employment", label: "Employment" },
  { value: "consulting", label: "Consulting" },
  { value: "part_time", label: "Part-time work" },
];

export const INCOME_FREQUENCY_OPTIONS: {
  value: IncomeFrequency;
  label: string;
}[] = [
  { value: "monthly", label: "Monthly" },
  { value: "annual", label: "Annual" },
];

export const TAXABILITY_OPTIONS: { value: Taxability; label: string }[] = [
  { value: "taxable", label: "Fully taxable" },
  { value: "partially_taxable", label: "Partially taxable" },
  { value: "tax_free", label: "Tax-free" },
];

export const INCOME_OWNER_OPTIONS: { value: IncomeOwner; label: string }[] = [
  { value: "self", label: "Self" },
  { value: "spouse", label: "Spouse" },
  { value: "joint", label: "Joint" },
];

export function incomeTypeLabel(t: IncomeType): string {
  return INCOME_TYPE_OPTIONS.find((o) => o.value === t)?.label ?? t;
}

export function taxabilityLabel(t: Taxability): string {
  return TAXABILITY_OPTIONS.find((o) => o.value === t)?.label ?? t;
}

export function incomeOwnerLabel(o: IncomeOwner): string {
  return INCOME_OWNER_OPTIONS.find((x) => x.value === o)?.label ?? o;
}
