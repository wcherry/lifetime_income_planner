import type {
  CashFlowDirection,
  EventRecurrence,
  LifeEventType,
} from "../api/types";

export const LIFE_EVENT_TYPE_OPTIONS: { value: LifeEventType; label: string }[] =
  [
    { value: "sell_house", label: "Sell house" },
    { value: "buy_home", label: "Buy home / property" },
    { value: "inheritance", label: "Inheritance" },
    { value: "downsize", label: "Downsize" },
    { value: "start_medicare", label: "Start Medicare" },
    { value: "claim_social_security", label: "Claim Social Security" },
    { value: "pay_off_mortgage", label: "Pay off mortgage" },
    { value: "relocate", label: "Move to another state" },
    { value: "large_purchase", label: "Large purchase / vacation" },
    { value: "gift", label: "Gift" },
    { value: "death_of_spouse", label: "Death of spouse" },
    { value: "other", label: "Other" },
  ];

export const DIRECTION_OPTIONS: { value: CashFlowDirection; label: string }[] = [
  { value: "inflow", label: "Money in" },
  { value: "outflow", label: "Money out" },
];

export const RECURRENCE_OPTIONS: { value: EventRecurrence; label: string }[] = [
  { value: "one_time", label: "One-time" },
  { value: "monthly", label: "Monthly" },
  { value: "annual", label: "Annual" },
];

export function lifeEventTypeLabel(t: LifeEventType): string {
  return LIFE_EVENT_TYPE_OPTIONS.find((o) => o.value === t)?.label ?? t;
}

export function directionLabel(d: CashFlowDirection): string {
  return DIRECTION_OPTIONS.find((o) => o.value === d)?.label ?? d;
}

export function recurrenceLabel(r: EventRecurrence): string {
  return RECURRENCE_OPTIONS.find((o) => o.value === r)?.label ?? r;
}
