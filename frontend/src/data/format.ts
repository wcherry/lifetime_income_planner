const currency = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

export function formatCurrency(value: number): string {
  return currency.format(value);
}

export function formatPercent(value: number): string {
  return `${value.toFixed(1)}%`;
}

/** Currency with an explicit +/− sign, for signed cash flows. */
export function formatSignedCurrency(value: number): string {
  const abs = formatCurrency(Math.abs(value));
  return value < 0 ? `−${abs}` : `+${abs}`;
}

/** Compact currency for dense contexts (axis ticks, chips): $250K, $1.2M. */
export function formatCompactCurrency(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 1_000_000) {
    return `$${(value / 1_000_000).toFixed(abs >= 10_000_000 ? 0 : 1)}M`;
  }
  if (abs >= 1_000) {
    return `$${Math.round(value / 1_000)}K`;
  }
  return `$${Math.round(value)}`;
}
