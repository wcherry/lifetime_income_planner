import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { YearProjection } from "../api/types";
import { axisTicks } from "../data/chart";
import { formatCompactCurrency, formatCurrency, formatSignedCurrency } from "../data/format";
import { lifeEventsTone } from "../data/projection";

interface Props {
  annual: YearProjection[];
  depletionYear: number | null;
}

const HEIGHT = 280;
// Extra top padding reserves a lane above the plot for milestone flags.
const PAD = { top: 36, right: 72, bottom: 28, left: 60 };
const MILESTONE_LANE_Y = 15;

/**
 * Single-series area + line chart of projected net worth (each year's ending
 * balance) over time. Dependency-free inline SVG: it measures its container and
 * redraws responsively, with a crosshair + tooltip that snaps to the nearest
 * year. One series, so there is no legend — the card title names it.
 */
export function NetWorthChart({ annual, depletionYear }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(640);
  const [hover, setHover] = useState<number | null>(null);
  const [hoverEvent, setHoverEvent] = useState<number | null>(null);
  const [hoverMilestone, setHoverMilestone] = useState<number | null>(null);

  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    setHover(null);
    setHoverEvent(null);
    setHoverMilestone(null);
  }, [annual]);

  if (annual.length === 0) return null;

  const plotW = Math.max(width - PAD.left - PAD.right, 10);
  const plotH = HEIGHT - PAD.top - PAD.bottom;

  const startYear = annual[0].year;
  const endYear = annual[annual.length - 1].year;
  const yearSpan = Math.max(endYear - startYear, 1);
  const dataMax = Math.max(...annual.map((y) => y.ending_balance), 0);
  const { niceMax, ticks } = axisTicks(dataMax, 4);
  const safeMax = niceMax || 1;

  const xFor = (year: number) => PAD.left + ((year - startYear) / yearSpan) * plotW;
  const yFor = (value: number) => PAD.top + plotH - (value / safeMax) * plotH;

  const pts = annual.map((y) => ({ x: xFor(y.year), y: yFor(y.ending_balance), d: y }));
  const linePath = pts.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.y}`).join(" ");
  const areaPath =
    `M${pts[0].x},${PAD.top + plotH} ` +
    pts.map((p) => `L${p.x},${p.y}`).join(" ") +
    ` L${pts[pts.length - 1].x},${PAD.top + plotH} Z`;

  // X-axis year labels: up to five evenly spaced across the span, endpoints
  // exact. De-duplicated so a short span never collides two labels.
  const xCount = Math.min(4, yearSpan);
  const xLabels = Array.from(
    new Set(
      Array.from({ length: xCount + 1 }, (_, i) => Math.round(startYear + (i * yearSpan) / xCount)),
    ),
  );

  const last = pts[pts.length - 1];
  const depletionPt =
    depletionYear != null ? (pts.find((p) => p.d.year === depletionYear) ?? null) : null;

  // A "$" badge for each year that has life events, floated just above the line,
  // green for a net inflow and red for a net outflow.
  const eventMarkers = pts
    .filter((p) => p.d.life_events.length > 0)
    .map((p) => ({
      x: p.x,
      y: Math.min(Math.max(p.y - 18, PAD.top + 10), PAD.top + plotH - 10),
      tone: lifeEventsTone(p.d.life_events),
      events: p.d.life_events,
    }));

  // Milestone flags sit in the lane above the plot, one per year with any.
  const milestoneMarkers = pts
    .filter((p) => p.d.milestones.length > 0)
    .map((p) => ({ x: p.x, milestones: p.d.milestones }));

  function onMove(e: React.PointerEvent<SVGSVGElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const yr = startYear + Math.round(((px - PAD.left) / plotW) * yearSpan);
    const clamped = Math.min(endYear, Math.max(startYear, yr));
    const idx = annual.findIndex((y) => y.year === clamped);
    setHover(idx >= 0 ? idx : null);
  }

  // A marker tooltip takes precedence over the crosshair to avoid two tooltips.
  const activeEvent = hoverEvent != null ? eventMarkers[hoverEvent] : null;
  const activeMilestone = hoverMilestone != null ? milestoneMarkers[hoverMilestone] : null;
  const hoverPt = hover != null && !activeEvent && !activeMilestone ? pts[hover] : null;
  // Keep the tooltip inside the container.
  const tipLeft = hoverPt ? Math.min(Math.max(hoverPt.x, 70), width - 70) : 0;
  const eventTipLeft = activeEvent ? Math.min(Math.max(activeEvent.x, 80), width - 80) : 0;
  const milestoneTipLeft = activeMilestone
    ? Math.min(Math.max(activeMilestone.x, 90), width - 90)
    : 0;

  return (
    <div className="chart" ref={containerRef}>
      <svg
        width={width}
        height={HEIGHT}
        role="img"
        aria-label="Projected net worth over time"
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

        {/* Area + line */}
        <path className="chart-area" d={areaPath} />
        <path className="chart-line" d={linePath} />

        {/* Depletion marker */}
        {depletionPt && (
          <g>
            <line
              className="chart-depletion"
              x1={depletionPt.x}
              y1={PAD.top}
              x2={depletionPt.x}
              y2={PAD.top + plotH}
            />
            <circle className="chart-depletion-dot" cx={depletionPt.x} cy={depletionPt.y} r={4} />
          </g>
        )}

        {/* Endpoint marker + direct label */}
        <circle className="chart-end-dot" cx={last.x} cy={last.y} r={4} />
        <text className="chart-end-label" x={last.x + 8} y={last.y} dy="0.32em">
          {formatCompactCurrency(last.d.ending_balance)}
        </text>

        {/* Life-event markers */}
        {eventMarkers.map((m, i) => (
          <g
            key={i}
            className={`chart-event chart-event-${m.tone}`}
            onPointerEnter={() => setHoverEvent(i)}
            onPointerLeave={() => setHoverEvent((cur) => (cur === i ? null : cur))}
          >
            {/* Enlarged transparent hit target */}
            <circle className="chart-event-hit" cx={m.x} cy={m.y} r={12} />
            <circle className="chart-event-dot" cx={m.x} cy={m.y} r={9} />
            <text className="chart-event-glyph" x={m.x} y={m.y} dy="0.32em" textAnchor="middle">
              $
            </text>
          </g>
        ))}

        {/* Milestone flags (top lane) */}
        {milestoneMarkers.map((m, i) => (
          <g
            key={i}
            className="chart-milestone"
            onPointerEnter={() => setHoverMilestone(i)}
            onPointerLeave={() => setHoverMilestone((cur) => (cur === i ? null : cur))}
          >
            {/* A thin guide line down to the plot helps place the year */}
            <line
              className="chart-milestone-guide"
              x1={m.x}
              y1={MILESTONE_LANE_Y + 7}
              x2={m.x}
              y2={PAD.top + plotH}
            />
            <circle className="chart-milestone-hit" cx={m.x} cy={MILESTONE_LANE_Y} r={11} />
            {/* Flag: pole + pennant */}
            <line
              className="chart-milestone-pole"
              x1={m.x}
              y1={MILESTONE_LANE_Y - 7}
              x2={m.x}
              y2={MILESTONE_LANE_Y + 7}
            />
            <path
              className="chart-milestone-flag"
              d={`M${m.x},${MILESTONE_LANE_Y - 7} L${m.x + 9},${MILESTONE_LANE_Y - 4.5} L${m.x},${MILESTONE_LANE_Y - 2} Z`}
            />
          </g>
        ))}

        {/* Hover crosshair + point */}
        {hoverPt && (
          <g>
            <line
              className="chart-crosshair"
              x1={hoverPt.x}
              y1={PAD.top}
              x2={hoverPt.x}
              y2={PAD.top + plotH}
            />
            <circle className="chart-hover-dot" cx={hoverPt.x} cy={hoverPt.y} r={4} />
          </g>
        )}
      </svg>

      {hoverPt && (
        <div className="chart-tooltip" style={{ left: tipLeft }}>
          <span className="chart-tooltip-value">{formatCurrency(hoverPt.d.ending_balance)}</span>
          <span className="chart-tooltip-meta">
            {hoverPt.d.year} · age {hoverPt.d.primary_age}
            {hoverPt.d.spouse_age != null && ` / ${hoverPt.d.spouse_age}`}
          </span>
        </div>
      )}

      {activeEvent && (
        <div
          className="chart-tooltip chart-event-tooltip"
          style={{ left: eventTipLeft, top: activeEvent.y - 12 }}
        >
          {activeEvent.events.map((e, i) => (
            <span className="chart-event-row" key={i}>
              <span className="chart-event-name">{e.name}</span>
              <span className={`chart-event-amt chart-event-amt-${e.amount < 0 ? "out" : "in"}`}>
                {formatSignedCurrency(e.amount)}
              </span>
            </span>
          ))}
        </div>
      )}

      {activeMilestone && (
        <div
          className="chart-tooltip chart-milestone-tooltip"
          style={{ left: milestoneTipLeft, top: MILESTONE_LANE_Y + 14 }}
        >
          {activeMilestone.milestones.map((m, i) => (
            <span className="chart-milestone-row" key={i}>
              <span className="chart-milestone-label">{m.label}</span>
              <span className="chart-milestone-detail">{m.detail}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
