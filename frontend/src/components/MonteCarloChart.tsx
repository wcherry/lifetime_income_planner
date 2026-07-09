import { useLayoutEffect, useRef, useState } from "react";
import type { MonteCarloYearBand } from "../api/types";
import { axisTicks } from "../data/chart";
import { formatCompactCurrency } from "../data/format";

interface Props {
  bands: MonteCarloYearBand[];
}

const HEIGHT = 280;
const PAD = { top: 16, right: 72, bottom: 28, left: 60 };

/**
 * Percentile "fan chart" for Monte Carlo simulation results: a light outer
 * band (p10–p90, the 80% confidence interval), a darker inner band (p25–p75,
 * the 50% confidence interval), and a solid median line. Dependency-free
 * inline SVG, following the same measure/scale/crosshair pattern as
 * NetWorthChart — simpler, since there are no life-event/milestone markers.
 */
export function MonteCarloChart({ bands }: Props) {
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

  if (bands.length === 0) return null;

  const plotW = Math.max(width - PAD.left - PAD.right, 10);
  const plotH = HEIGHT - PAD.top - PAD.bottom;

  const startYear = bands[0].year;
  const endYear = bands[bands.length - 1].year;
  const yearSpan = Math.max(endYear - startYear, 1);
  const dataMax = Math.max(...bands.map((b) => b.p90), 0);
  const { niceMax, ticks } = axisTicks(dataMax, 4);
  const safeMax = niceMax || 1;

  const xFor = (year: number) => PAD.left + ((year - startYear) / yearSpan) * plotW;
  const yFor = (value: number) => PAD.top + plotH - (value / safeMax) * plotH;

  const pts = bands.map((b) => ({
    x: xFor(b.year),
    yP10: yFor(b.p10),
    yP25: yFor(b.p25),
    yMedian: yFor(b.p50),
    yP75: yFor(b.p75),
    yP90: yFor(b.p90),
    d: b,
  }));

  // Each band is a closed path: forward along its upper edge, back along its
  // lower edge.
  const outerBandPath =
    pts.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.yP90}`).join(" ") +
    " " +
    [...pts]
      .reverse()
      .map((p) => `L${p.x},${p.yP10}`)
      .join(" ") +
    " Z";

  const innerBandPath =
    pts.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.yP75}`).join(" ") +
    " " +
    [...pts]
      .reverse()
      .map((p) => `L${p.x},${p.yP25}`)
      .join(" ") +
    " Z";

  const medianPath = pts.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.yMedian}`).join(" ");

  // X-axis year labels: up to five evenly spaced across the span, endpoints
  // exact. De-duplicated so a short span never collides two labels.
  const xCount = Math.min(4, yearSpan);
  const xLabels = Array.from(
    new Set(
      Array.from({ length: xCount + 1 }, (_, i) => Math.round(startYear + (i * yearSpan) / xCount)),
    ),
  );

  function onMove(e: React.PointerEvent<SVGSVGElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const yr = startYear + Math.round(((px - PAD.left) / plotW) * yearSpan);
    const clamped = Math.min(endYear, Math.max(startYear, yr));
    const idx = bands.findIndex((b) => b.year === clamped);
    setHover(idx >= 0 ? idx : null);
  }

  const hoverPt = hover != null ? pts[hover] : null;
  // Keep the tooltip inside the container.
  const tipLeft = hoverPt ? Math.min(Math.max(hoverPt.x, 90), width - 90) : 0;

  return (
    <div className="chart" ref={containerRef}>
      <svg
        width={width}
        height={HEIGHT}
        role="img"
        aria-label="Monte Carlo simulation outcome percentiles over time"
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

        {/* X-axis year labels */}
        {xLabels.map((yr) => (
          <text key={yr} className="chart-tick" x={xFor(yr)} y={HEIGHT - 8} textAnchor="middle">
            {yr}
          </text>
        ))}

        {/* p10–p90 (80% CI) and p25–p75 (50% CI) bands, median line on top */}
        <path className="mc-chart-band-outer" d={outerBandPath} />
        <path className="mc-chart-band-inner" d={innerBandPath} />
        <path className="chart-line" d={medianPath} />

        {/* Hover crosshair + point on the median line */}
        {hoverPt && (
          <g>
            <line
              className="chart-crosshair"
              x1={hoverPt.x}
              y1={PAD.top}
              x2={hoverPt.x}
              y2={PAD.top + plotH}
            />
            <circle className="chart-hover-dot" cx={hoverPt.x} cy={hoverPt.yMedian} r={4} />
          </g>
        )}
      </svg>

      {hoverPt && (
        <div className="chart-tooltip" style={{ left: tipLeft }}>
          <span className="chart-tooltip-value">
            {hoverPt.d.year} — Median {formatCompactCurrency(hoverPt.d.p50)}
          </span>
          <span className="chart-tooltip-meta">
            Range {formatCompactCurrency(hoverPt.d.p10)}–{formatCompactCurrency(hoverPt.d.p90)}
          </span>
          <span className="chart-tooltip-meta">
            25th–75th {formatCompactCurrency(hoverPt.d.p25)}–{formatCompactCurrency(hoverPt.d.p75)}
          </span>
        </div>
      )}
    </div>
  );
}
