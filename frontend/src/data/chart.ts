// Small, dependency-free helpers for laying out the net worth chart's axes.

/** Round a number to a "nice" value (1, 2, 5, 10 × 10ⁿ), per Heckbert's
 *  loose-labeling algorithm. */
function niceNum(value: number, round: boolean): number {
  const exp = Math.floor(Math.log10(value));
  const frac = value / 10 ** exp;
  let niceFrac: number;
  if (round) {
    niceFrac = frac < 1.5 ? 1 : frac < 3 ? 2 : frac < 7 ? 5 : 10;
  } else {
    niceFrac = frac <= 1 ? 1 : frac <= 2 ? 2 : frac <= 5 ? 5 : 10;
  }
  return niceFrac * 10 ** exp;
}

/**
 * Produce a rounded axis maximum and evenly spaced ticks (including 0) that
 * comfortably cover `dataMax` in about `count` steps.
 */
export function axisTicks(dataMax: number, count = 4): { niceMax: number; ticks: number[] } {
  if (!Number.isFinite(dataMax) || dataMax <= 0) {
    return { niceMax: 0, ticks: [0] };
  }
  const range = niceNum(dataMax, false);
  const step = niceNum(range / count, true);
  const niceMax = Math.ceil(dataMax / step) * step;
  const ticks: number[] = [];
  for (let v = 0; v <= niceMax + step / 2; v += step) {
    ticks.push(Math.round(v));
  }
  return { niceMax, ticks };
}
