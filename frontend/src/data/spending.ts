import type { SpendingCategory, SpendingFrequency } from "../api/types";

export const SPENDING_CATEGORY_OPTIONS: {
  value: SpendingCategory;
  label: string;
}[] = [
  { value: "essential", label: "Essential" },
  { value: "discretionary", label: "Discretionary" },
  { value: "healthcare", label: "Healthcare" },
  { value: "travel", label: "Travel" },
  { value: "one_time", label: "One-time expense" },
  { value: "charity", label: "Charity" },
  { value: "taxes", label: "Taxes" },
  { value: "home_maintenance", label: "Home maintenance" },
  { value: "vehicle_replacement", label: "Vehicle replacement" },
  { value: "large_purchase", label: "Large purchase" },
];

export const SPENDING_FREQUENCY_OPTIONS: {
  value: SpendingFrequency;
  label: string;
}[] = [
  { value: "monthly", label: "Monthly" },
  { value: "annual", label: "Annual" },
  { value: "one_time", label: "One-time" },
];

export function spendingCategoryLabel(c: SpendingCategory): string {
  return SPENDING_CATEGORY_OPTIONS.find((o) => o.value === c)?.label ?? c;
}

export function spendingFrequencyLabel(f: SpendingFrequency): string {
  return SPENDING_FREQUENCY_OPTIONS.find((o) => o.value === f)?.label ?? f;
}
