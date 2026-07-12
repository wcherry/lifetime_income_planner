import { useLayoutEffect, useRef, useState } from "react";
import type { SpendingTrackerCategoryMonthSeries } from "../api/types";
import { axisTicks } from "../data/chart";
import { formatCompactCurrency, formatCurrencyCents } from "../data/format";
import { buildYearChartSeries, formatMonthLabel } from "../data/spendingTracker";

interface Props {
  year: number;
  categories: SpendingTrackerCategoryMonthSeries[];
}

const HEIGHT = 240;
const PAD = { top: 20, right: 16, bottom: 28, left: 60 };
const GAP_PX = 2;

/**
 * Stacked area chart of categorized expenses by month across a calendar
 * year, one band per category (folding overflow into "Other" past eight
 * series). Dependency-free inline SVG, matching the app's other charts'
 * conventions. Always legend + tooltip labeled, per the categorical palette's
 * secondary-encoding requirement.
 */
export function SpendingTrackerYearChart({ year, categories }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(640);
  const [hover, setHover] = useState<number | null>(null);

  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const series = buildYearChartSeries(categories);
  if (series.length === 0) return null;

  const plotW = Math.max(width - PAD.left - PAD.right, 10);
  const plotH = HEIGHT - PAD.top - PAD.bottom;
  const monthSpan = 11;

  // cumulative[i][m] = total of series[0..=i] for month m; cumulative[-1] is
  // an implicit all-zero baseline.
  const cumulative: number[][] = [];
  series.forEach((s, i) => {
    cumulative.push(s.monthlyTotals.map((v, m) => v + (i > 0 ? cumulative[i - 1][m] : 0)));
  });
  const totals = cumulative[cumulative.length - 1];

  const { niceMax, ticks } = axisTicks(Math.max(...totals, 0), 4);
  const safeMax = niceMax || 1;

  const xFor = (index: number) => PAD.left + (index / monthSpan) * plotW;
  const yFor = (value: number) => PAD.top + plotH - (value / safeMax) * plotH;
  const baselineY = PAD.top + plotH;

  const bandPath = (i: number) => {
    const top = cumulative[i];
    const bottom = i > 0 ? cumulative[i - 1] : null;
    const topPts = top.map((v, m) => `${m === 0 ? "M" : "L"}${xFor(m)},${yFor(v)}`).join(" ");
    const bottomPts = bottom
      ? [...bottom].reverse().map((v, ri) => `L${xFor(11 - ri)},${yFor(v)}`).join(" ")
      : `L${xFor(11)},${baselineY} L${xFor(0)},${baselineY}`;
    return `${topPts} ${bottomPts} Z`;
  };

  const boundaryPath = (i: number) =>
    cumulative[i].map((v, m) => `${m === 0 ? "M" : "L"}${xFor(m)},${yFor(v)}`).join(" ");

  function onMove(e: React.PointerEvent<SVGSVGElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const idx = Math.round(((px - PAD.left) / plotW) * monthSpan);
    setHover(Math.min(11, Math.max(0, idx)));
  }

  const hoverX = hover != null ? xFor(hover) : null;
  const tipLeft = hoverX != null ? Math.min(Math.max(hoverX, 90), width - 90) : 0;
  const lastMonthTotal = totals[11];

  return (
    <div className="chart" ref={containerRef}>
      <svg
        width={width}
        height={HEIGHT}
        role="img"
        aria-label={`Expenses for ${year}`}
        onPointerMove={onMove}
        onPointerLeave={() => setHover(null)}
      >
        {/* Horizontal gridlines + y-axis ticks */}
        {ticks.map((t) => {
          const y = yFor(t);
          return (
            <g key={t}>
              <line className="chart-grid" x1={PAD.left} y1={y} x2={PAD.left + plotW} y2={y} />
              <text className="chart-tick" x={PAD.left - 8} y={y} dy="0.32em" textAnchor="end">
                {formatCompactCurrency(t)}
              </text>
            </g>
          );
        })}

        {/* X-axis month labels */}
        {Array.from({ length: 12 }, (_, m) => (
          <text
            key={m}
            className="chart-tick"
            x={xFor(m)}
            y={HEIGHT - 8}
            textAnchor="middle"
          >
            {formatMonthLabel(year, m + 1).split(" ")[0].slice(0, 3)}
          </text>
        ))}

        {/* Stacked bands, bottom to top. Solid fills, not a decorative
            wash — here the fill is the primary encoding (as with a stacked
            bar), so it keeps the validated palette's contrast intact. */}
        {series.map((s, i) => (
          <path key={s.id} d={bandPath(i)} fill={s.color} />
        ))}

        {/* Surface-color gap between touching bands (never a border) */}
        {series.slice(0, -1).map((_, i) => (
          <path
            key={`gap-${i}`}
            d={boundaryPath(i)}
            fill="none"
            stroke="var(--surface)"
            strokeWidth={GAP_PX}
          />
        ))}

        {/* Total outline along the top of the stack — neutral ink, not a
            series color, since it marks the sum rather than an identity. */}
        <path
          d={boundaryPath(series.length - 1)}
          fill="none"
          stroke="var(--text)"
          strokeOpacity={0.5}
          strokeWidth={1.5}
        />

        {/* Endpoint label: total for the final month */}
        <circle className="chart-end-dot" cx={xFor(11)} cy={yFor(lastMonthTotal)} r={4} />
        <text className="chart-end-label" x={xFor(11) - 8} y={yFor(lastMonthTotal) - 10} textAnchor="end">
          {formatCompactCurrency(lastMonthTotal)}
        </text>

        {/* Hover crosshair */}
        {hoverX != null && (
          <line className="chart-crosshair" x1={hoverX} y1={PAD.top} x2={hoverX} y2={baselineY} />
        )}
      </svg>

      {hover != null && (
        <div className="chart-tooltip" style={{ left: tipLeft }}>
          <span className="chart-tooltip-meta">{formatMonthLabel(year, hover + 1)}</span>
          {[...series].reverse().map((s) => (
            <span className="chart-stack-tooltip-row" key={s.id}>
              <span className="chart-stack-tooltip-key" style={{ background: s.color }} />
              <span className="chart-stack-tooltip-name">{s.name}</span>
              <span className="chart-stack-tooltip-value">
                {formatCurrencyCents(s.monthlyTotals[hover])}
              </span>
            </span>
          ))}
          <span className="chart-stack-tooltip-row chart-stack-tooltip-total">
            <span className="chart-stack-tooltip-name">Total</span>
            <span className="chart-stack-tooltip-value">{formatCurrencyCents(totals[hover])}</span>
          </span>
        </div>
      )}

      <div className="chart-legend">
        {series.map((s) => (
          <span className="chart-legend-item" key={s.id}>
            <span className="chart-legend-swatch" style={{ background: s.color }} />
            {s.name}
          </span>
        ))}
      </div>
    </div>
  );
}
