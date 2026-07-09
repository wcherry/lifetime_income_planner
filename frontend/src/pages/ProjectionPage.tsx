import { useEffect, useState } from "react";
import { api, ApiError } from "../api/client";
import type {
  EstimatedTaxes,
  MonteCarloResult,
  OptimizationCandidate,
  OptimizationGoal,
  OptimizeResponse,
  Projection,
  ProjectionSummary,
  WithdrawalStrategy,
  YearAca,
  YearIrmaa,
  YearProjection,
  YearTax,
} from "../api/types";
import { Alert, Button, Card, Field, Select, TextInput } from "../components/ui";
import { MonteCarloChart } from "../components/MonteCarloChart";
import { NetWorthChart } from "../components/NetWorthChart";
import {
  categoryLabel,
  formatCurrency,
  formatPercent,
  formatRate,
  formatSignedCurrency,
  hasWithdrawals,
  lifetimeEffectiveRate,
  planOutlook,
  rmdExceedsSpendingBy,
} from "../data/projection";

export function ProjectionPage() {
  const [projection, setProjection] = useState<Projection | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [needsProfile, setNeedsProfile] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);

  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const p = await api.getProjection();
        if (active) setProjection(p);
      } catch (err) {
        if (!active) return;
        // A missing profile is an expected, actionable state, not an error.
        if (err instanceof ApiError && err.status === 400) {
          setNeedsProfile(true);
        } else {
          setError(err instanceof Error ? err.message : "Failed to load projection");
        }
      } finally {
        if (active) setLoading(false);
      }
    }
    load();
    return () => {
      active = false;
    };
  }, []);

  if (loading) return <p className="muted">Building your projection…</p>;

  if (needsProfile) {
    return (
      <Card title="Retirement projection">
        <p className="muted center">
          Create your retirement profile first — it sets the horizon for every projection. Then add
          your accounts, income, and spending to see your quarterly withdrawal plan.
        </p>
      </Card>
    );
  }

  if (error) return <Alert kind="error">{error}</Alert>;
  if (!projection) return null;

  const { summary, assumptions, annual, quarterly, estimated_taxes } = projection;
  const outlook = planOutlook(summary);
  const showWithdrawals = hasWithdrawals(projection);
  const effectiveRate = lifetimeEffectiveRate(projection);
  // The current year's tax detail drives the tax-breakdown card.
  const currentTax = annual[0]?.tax;
  // Roth conversions (feature 6) only surface once the strategy is in use.
  const showRoth = summary.total_lifetime_roth_conversions > 0;
  // The optimized strategy (feature 9) reorders accounts per year; only worth
  // a dedicated column once it's actually in use.
  const showWithdrawalOrder = assumptions.withdrawal_strategy === "tax_optimized";
  // ACA subsidy (Phase 3, feature 1): show the detail whenever a benchmark
  // premium is configured, and the lifetime tile once any subsidy is received.
  const showAca = assumptions.aca_benchmark_annual_premium > 0;
  const currentAca = annual[0]?.aca;
  // Medicare Part B premiums (Phase 3, feature 3): only worth a dedicated
  // column once the plan actually reaches an age where they're charged.
  const showMedicare = summary.total_lifetime_medicare_premiums > 0;
  // Medicare IRMAA surcharge (Phase 3, feature 4): only worth surfacing once
  // the plan's MAGI actually crosses a bracket.
  const showIrmaa = summary.total_lifetime_irmaa_surcharges > 0;
  const currentIrmaa = annual[0]?.irmaa;

  async function handleDownloadTaxReport() {
    setDownloadError(null);
    setDownloading(true);
    try {
      await api.downloadTaxSummaryCsv();
    } catch (err) {
      setDownloadError(err instanceof Error ? err.message : "Failed to download tax report");
    } finally {
      setDownloading(false);
    }
  }

  return (
    <div className="stack projection-page">
      <div className="page-head">
        <div>
          <h1>Projection</h1>
          <p className="muted">
            {projection.start_year}–{projection.end_year} · driven by{" "}
            {formatPercent(assumptions.inflation_rate)} inflation and your per-account returns
            {assumptions.is_default && " · using default assumptions"}
          </p>
        </div>
      </div>

      {/* Summary tiles */}
      <div className="tile-grid">
        <div className="tile">
          <span className="tile-label">Net worth today</span>
          <span className="tile-value">{formatCurrency(summary.current_net_worth)}</span>
        </div>
        <div className="tile">
          <span className="tile-label">Projected estate ({projection.end_year})</span>
          <span className="tile-value">{formatCurrency(summary.projected_ending_balance)}</span>
        </div>
        <div className={`tile tile-${outlook.tone}`}>
          <span className="tile-label">Outlook</span>
          <span className="tile-value tile-value-sm">{outlook.text}</span>
        </div>
        <div className="tile">
          <span className="tile-label">Lifetime withdrawals</span>
          <span className="tile-value">{formatCurrency(summary.total_lifetime_withdrawals)}</span>
        </div>
        <div className="tile">
          <span className="tile-label">Lifetime taxes</span>
          <span className="tile-value">{formatCurrency(summary.total_lifetime_taxes)}</span>
          <span className="tile-sub muted">≈ {formatRate(effectiveRate)} effective</span>
        </div>
        {showRoth && (
          <div className="tile">
            <span className="tile-label">Roth conversions</span>
            <span className="tile-value">
              {formatCurrency(summary.total_lifetime_roth_conversions)}
            </span>
            <span className="tile-sub muted">traditional → Roth over the plan</span>
          </div>
        )}
        {summary.total_lifetime_aca_subsidies > 0 && (
          <div className="tile tile-good">
            <span className="tile-label">ACA subsidies</span>
            <span className="tile-value">
              {formatCurrency(summary.total_lifetime_aca_subsidies)}
            </span>
            <span className="tile-sub muted">lifetime premium tax credits</span>
          </div>
        )}
        {showMedicare && (
          <div className="tile">
            <span className="tile-label">Medicare Part B</span>
            <span className="tile-value">
              {formatCurrency(summary.total_lifetime_medicare_premiums)}
            </span>
            <span className="tile-sub muted">lifetime premiums from age 65</span>
          </div>
        )}
        {showIrmaa && (
          <div className="tile">
            <span className="tile-label">IRMAA surcharges</span>
            <span className="tile-value">
              {formatCurrency(summary.total_lifetime_irmaa_surcharges)}
            </span>
            <span className="tile-sub muted">lifetime Part B + D income surcharge</span>
          </div>
        )}
      </div>

      {/* Net worth projection chart (feature 10) */}
      <Card title="Net worth over time" collapsible>
        <p className="muted">
          Projected total account balance at the end of each year
          {summary.depletion_year != null && ", with the shortfall year marked in red"}. Hover a{" "}
          <span className="legend-key legend-key-in">$</span> life event,{" "}
          <span className="legend-key legend-key-flag">⚑</span> milestone, or{" "}
          <span className="legend-key legend-key-rmd">⚠</span> RMD warning for details.
        </p>
        <NetWorthChart annual={annual} depletionYear={summary.depletion_year} />
      </Card>

      {/* Interactive what-if controls (Phase 4, feature 3) */}
      <WhatIfCard baseline={projection} />

      {/* Optimization goals (Phase 4, feature 5) */}
      <OptimizeCard />

      {/* Monte Carlo simulation (Phase 4, feature 6) */}
      <MonteCarloCard />

      {/* Quarterly withdrawal schedule (feature 9) */}
      <Card title={`Withdrawal schedule · ${projection.start_year}`} collapsible>
        <p className="muted">What to withdraw from which account next, taxable balances first.</p>
        {!showWithdrawals ? (
          <p className="muted center">
            Your income covers your spending this year — no withdrawals needed.
          </p>
        ) : (
          <div className="quarter-grid">
            {quarterly.map((q) => (
              <div className="quarter-card" key={q.quarter}>
                <div className="quarter-head">
                  <span className="quarter-label">{q.label}</span>
                  <span className="quarter-total">{formatCurrency(q.total_withdrawal)}</span>
                </div>
                <ul className="quarter-lines">
                  {q.withdrawals.map((w) => (
                    <li key={w.account_id}>
                      <span className="quarter-acct">{w.account_name}</span>
                      <span className="muted quarter-cat">{categoryLabel(w.category)}</span>
                      <span className="quarter-amt">{formatCurrency(w.amount)}</span>
                    </li>
                  ))}
                </ul>
                {q.estimated_tax > 0 && (
                  <div className="quarter-foot muted">
                    <span>Est. tax</span>
                    <span className="quarter-amt">{formatCurrency(q.estimated_tax)}</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* Estimated quarterly taxes (Phase 2, feature 7) */}
      <EstimatedTaxesCard estimated={estimated_taxes} />

      {/* Tax breakdown for the current year (Phase 2, features 1–5) */}
      {currentTax && (
        <TaxBreakdownCard
          tax={currentTax}
          rothConversion={annual[0]?.roth_conversion ?? 0}
          year={projection.start_year}
          isCurrentYear={projection.start_year === projection.current_year}
        />
      )}

      {/* Multi-year tax report + CSV export (Phase 2, feature 8) */}
      <TaxReportCard
        annual={annual}
        showWithdrawalOrder={showWithdrawalOrder}
        onDownload={handleDownloadTaxReport}
        downloading={downloading}
        downloadError={downloadError}
      />

      {/* ACA subsidy for the current year (Phase 3, feature 1) */}
      {showAca && currentAca && (
        <AcaSubsidyCard aca={currentAca} year={projection.start_year} />
      )}

      {/* Medicare IRMAA surcharge for the current year (Phase 3, feature 4) */}
      {showIrmaa && currentIrmaa && (
        <IrmaaCard irmaa={currentIrmaa} year={projection.start_year} />
      )}

      {/* Annual projection table (Phase 1, feature 8) */}
      <Card title="Year-by-year projection" collapsible>
        <div className="table-scroll">
          <table className="proj-table">
            <thead>
              <tr>
                <th>Year</th>
                <th>Age</th>
                <th className="num">Start</th>
                <th className="num">Income</th>
                <th className="num">Spending</th>
                {showMedicare && <th className="num">Medicare</th>}
                {showIrmaa && <th className="num">IRMAA</th>}
                <th className="num">Growth</th>
                <th className="num">Withdrawals</th>
                {showRoth && <th className="num">Roth conv.</th>}
                <th className="num">Taxes</th>
                <th className="num">End</th>
                <th>Events</th>
              </tr>
            </thead>
            <tbody>
              {annual.map((y) => {
                const rmdExcess = rmdExceedsSpendingBy(y);
                return (
                  <tr key={y.year} className={y.shortfall > 0 ? "row-warn" : ""}>
                    <td>{y.year}</td>
                    <td>
                      {y.primary_age}
                      {y.spouse_age != null && ` / ${y.spouse_age}`}
                    </td>
                    <td className="num">{formatCurrency(y.starting_balance)}</td>
                    <td className="num">{formatCurrency(y.income)}</td>
                    <td className="num">{formatCurrency(y.spending)}</td>
                    {showMedicare && (
                      <td className="num">
                        {y.medicare_premiums > 0 ? formatCurrency(y.medicare_premiums) : "—"}
                      </td>
                    )}
                    {showIrmaa && (
                      <td className="num">
                        {y.irmaa_surcharge > 0 ? formatCurrency(y.irmaa_surcharge) : "—"}
                      </td>
                    )}
                    <td className="num">{formatCurrency(y.growth)}</td>
                    <td className="num">{formatCurrency(y.withdrawals)}</td>
                    {showRoth && (
                      <td className="num">
                        {y.roth_conversion > 0 ? formatCurrency(y.roth_conversion) : "—"}
                      </td>
                    )}
                    <td className="num" title={`Effective rate ${formatRate(y.tax.effective_rate)}`}>
                      {formatCurrency(y.taxes)}
                    </td>
                    <td className="num">{formatCurrency(y.ending_balance)}</td>
                    <td>
                      {(y.life_events.length > 0 || y.milestones.length > 0 || rmdExcess > 0) && (
                        <span className="event-badges">
                          {rmdExcess > 0 && (
                            <span
                              className="rmd-badge"
                              title={`RMD of ${formatCurrency(y.rmd_amount)} exceeds spending by ${formatCurrency(rmdExcess)}`}
                            >
                              ⚠
                            </span>
                          )}
                          {y.life_events.map((e, i) => (
                            <span
                              key={`e${i}`}
                              className={`event-badge event-badge-${e.amount < 0 ? "out" : "in"}`}
                              title={`${e.name}: ${formatSignedCurrency(e.amount)}`}
                            >
                              $
                            </span>
                          ))}
                          {y.milestones.map((m, i) => (
                            <span
                              key={`m${i}`}
                              className="milestone-badge"
                              title={`${m.label} — ${m.detail}`}
                            >
                              ⚑
                            </span>
                          ))}
                        </span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Card>
    </div>
  );
}

/**
 * Interactive what-if controls (Phase 4, feature 3): sliders for inflation,
 * investment return, spending level, Social Security timing, and a one-time
 * market crash, recalculated live against the server without saving
 * anything. Debounces requests while a slider is being dragged and compares
 * the result against the plan's baseline projection already on the page.
 */
function WhatIfCard({ baseline }: { baseline: Projection }) {
  const baseInflation = baseline.assumptions.inflation_rate;
  const [inflationRate, setInflationRate] = useState(baseInflation);
  const [roiDelta, setRoiDelta] = useState(0);
  const [spendingPct, setSpendingPct] = useState(0);
  const [ssDelayYears, setSsDelayYears] = useState(0);
  const [marketCrashPct, setMarketCrashPct] = useState(0);
  const [result, setResult] = useState<Projection | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isBaseline =
    inflationRate === baseInflation &&
    roiDelta === 0 &&
    spendingPct === 0 &&
    ssDelayYears === 0 &&
    marketCrashPct === 0;

  useEffect(() => {
    if (isBaseline) {
      setResult(null);
      setError(null);
      return;
    }
    let active = true;
    const handle = setTimeout(async () => {
      setLoading(true);
      setError(null);
      try {
        const r = await api.runWhatIf({
          inflation_rate: inflationRate,
          investment_return_delta: roiDelta,
          spending_adjustment_pct: spendingPct,
          social_security_delay_years: ssDelayYears,
          market_crash_pct: marketCrashPct === 0 ? null : marketCrashPct,
        });
        if (active) setResult(r);
      } catch (err) {
        if (active) setError(err instanceof Error ? err.message : "Failed to recalculate");
      } finally {
        if (active) setLoading(false);
      }
    }, 400);
    return () => {
      active = false;
      clearTimeout(handle);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inflationRate, roiDelta, spendingPct, ssDelayYears, marketCrashPct]);

  function handleReset() {
    setInflationRate(baseInflation);
    setRoiDelta(0);
    setSpendingPct(0);
    setSsDelayYears(0);
    setMarketCrashPct(0);
  }

  return (
    <Card title="What-if" collapsible defaultOpen={false}>
      <p className="muted">
        Drag a slider to see how the plan reacts. Nothing is saved — this compares against your
        current plan below.
      </p>

      <div className="what-if-controls">
        <SliderControl
          label="Inflation"
          value={inflationRate}
          onChange={setInflationRate}
          min={-2}
          max={10}
          step={0.1}
          suffix="%"
          format={(v) => v.toFixed(1)}
        />
        <SliderControl
          label="Investment return"
          value={roiDelta}
          onChange={setRoiDelta}
          min={-10}
          max={10}
          step={0.5}
          suffix=" pts"
          signed
        />
        <SliderControl
          label="Annual spending"
          value={spendingPct}
          onChange={setSpendingPct}
          min={-50}
          max={50}
          step={1}
          suffix="%"
          signed
        />
        <SliderControl
          label="Social Security timing"
          value={ssDelayYears}
          onChange={setSsDelayYears}
          min={-5}
          max={5}
          step={1}
          suffix=" yrs"
          signed
        />
        <SliderControl
          label="Market crash (year 1)"
          value={marketCrashPct}
          onChange={setMarketCrashPct}
          min={-50}
          max={0}
          step={5}
          suffix=" pts"
        />
      </div>

      <div className="form-actions">
        <Button variant="ghost" onClick={handleReset} disabled={isBaseline}>
          Reset to baseline
        </Button>
        {loading && <span className="muted">Recalculating…</span>}
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {result && (
        <div className="what-if-compare">
          <WhatIfTile
            label="Estate"
            baseline={baseline.summary.projected_ending_balance}
            value={result.summary.projected_ending_balance}
          />
          <WhatIfTile
            label="Lifetime taxes"
            baseline={baseline.summary.total_lifetime_taxes}
            value={result.summary.total_lifetime_taxes}
            lowerIsBetter
          />
          <WhatIfTile
            label="Lifetime withdrawals"
            baseline={baseline.summary.total_lifetime_withdrawals}
            value={result.summary.total_lifetime_withdrawals}
            lowerIsBetter
          />
          <div className="what-if-compare-tile">
            <span className="tile-label">Money lasts</span>
            <span className="tile-value tile-value-sm">
              {result.summary.depletion_year != null
                ? `Runs short in ${result.summary.depletion_year}`
                : "Full plan"}
            </span>
          </div>
        </div>
      )}
    </Card>
  );
}

function SliderControl({
  label,
  value,
  onChange,
  min,
  max,
  step,
  suffix,
  signed,
  format,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  step: number;
  suffix: string;
  signed?: boolean;
  format?: (v: number) => string;
}) {
  const displayed = format ? format(value) : String(value);
  const sign = signed && value > 0 ? "+" : "";
  return (
    <div className="what-if-control">
      <div className="what-if-control-head">
        <span>{label}</span>
        <span className="what-if-control-value">
          {sign}
          {displayed}
          {suffix}
        </span>
      </div>
      <input
        type="range"
        className="what-if-slider"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
      />
    </div>
  );
}

/** One comparison tile: the what-if value plus its delta from baseline. */
function WhatIfTile({
  label,
  baseline,
  value,
  lowerIsBetter,
}: {
  label: string;
  baseline: number;
  value: number;
  lowerIsBetter?: boolean;
}) {
  const delta = value - baseline;
  const improved = lowerIsBetter ? delta <= 0 : delta >= 0;
  return (
    <div className="what-if-compare-tile">
      <span className="tile-label">{label}</span>
      <span className="tile-value tile-value-sm">{formatCurrency(value)}</span>
      {Math.abs(delta) >= 1 && (
        <span
          className={`what-if-compare-delta ${
            improved ? "what-if-compare-delta-up" : "what-if-compare-delta-down"
          }`}
        >
          {formatSignedCurrency(delta)} vs. current plan
        </span>
      )}
    </div>
  );
}

const GOAL_OPTIONS: { value: OptimizationGoal; label: string }[] = [
  { value: "minimize_taxes", label: "Minimize lifetime taxes" },
  { value: "maximize_estate", label: "Maximize estate (ending balance)" },
  { value: "maximize_plan_longevity", label: "Maximize plan longevity" },
  { value: "minimize_irmaa", label: "Minimize Medicare IRMAA surcharges" },
  { value: "maximize_aca_subsidy", label: "Maximize ACA subsidies" },
];

function strategyLabel(s: WithdrawalStrategy): string {
  return s === "tax_optimized" ? "Tax-optimized" : "Conventional";
}

function goalMetricLabel(goal: OptimizationGoal): string {
  switch (goal) {
    case "minimize_taxes":
      return "Lifetime taxes";
    case "maximize_estate":
      return "Estate";
    case "maximize_plan_longevity":
      return "Money lasts until";
    case "minimize_irmaa":
      return "Lifetime IRMAA";
    case "maximize_aca_subsidy":
      return "Lifetime ACA subsidy";
  }
}

function goalMetricValue(goal: OptimizationGoal, summary: ProjectionSummary): string {
  switch (goal) {
    case "minimize_taxes":
      return formatCurrency(summary.total_lifetime_taxes);
    case "maximize_estate":
      return formatCurrency(summary.projected_ending_balance);
    case "maximize_plan_longevity":
      return summary.depletion_year != null ? String(summary.depletion_year) : "Full plan";
    case "minimize_irmaa":
      return formatCurrency(summary.total_lifetime_irmaa_surcharges);
    case "maximize_aca_subsidy":
      return formatCurrency(summary.total_lifetime_aca_subsidies);
  }
}

/**
 * Optimization goals (Phase 4, feature 5): searches a small grid of
 * withdrawal-strategy / Roth-conversion-ceiling combinations against the
 * live working set — reusing the same projection engine as the what-if and
 * comparison features — and recommends whichever combination best serves the
 * chosen goal. "Apply to my plan" saves the recommendation as the user's
 * actual assumptions.
 */
function OptimizeCard() {
  const [goal, setGoal] = useState<OptimizationGoal>("minimize_taxes");
  const [result, setResult] = useState<OptimizeResponse | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);

  async function handleRun() {
    setError(null);
    setApplied(false);
    setRunning(true);
    try {
      const r = await api.optimizeProjection({ goal });
      setResult(r);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to run optimizer");
    } finally {
      setRunning(false);
    }
  }

  async function handleApply(candidate: OptimizationCandidate) {
    const ceilingText =
      candidate.roth_conversion_ceiling > 0
        ? formatCurrency(candidate.roth_conversion_ceiling) + " Roth ceiling"
        : "no Roth conversions";
    if (
      !window.confirm(
        `Apply ${strategyLabel(candidate.withdrawal_strategy)} withdrawals with ${ceilingText} to your saved assumptions?`,
      )
    )
      return;
    setApplying(true);
    setError(null);
    try {
      const current = await api.getAssumptions();
      await api.saveAssumptions({
        inflation_rate: current.inflation_rate,
        investment_return_rate: current.investment_return_rate,
        healthcare_inflation_rate: current.healthcare_inflation_rate,
        social_security_cola_rate: current.social_security_cola_rate,
        roth_conversion_ceiling: candidate.roth_conversion_ceiling,
        roth_conversion_start_year: null,
        roth_conversion_end_year: null,
        withdrawal_strategy: candidate.withdrawal_strategy,
        aca_benchmark_annual_premium: current.aca_benchmark_annual_premium,
        medicare_part_b_annual_premium: current.medicare_part_b_annual_premium,
      });
      setApplied(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to apply recommendation");
    } finally {
      setApplying(false);
    }
  }

  return (
    <Card title="Optimize" collapsible defaultOpen={false}>
      <p className="muted">
        Searches withdrawal strategy and Roth conversion levels against your current plan and
        recommends whichever combination best serves your goal.
      </p>

      <div className="grid-2">
        <Field label="Goal" htmlFor="optimize-goal">
          <Select
            id="optimize-goal"
            value={goal}
            onChange={(e) => setGoal(e.target.value as OptimizationGoal)}
          >
            {GOAL_OPTIONS.map((g) => (
              <option key={g.value} value={g.value}>
                {g.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>

      <div className="form-actions">
        <Button onClick={handleRun} disabled={running}>
          {running ? "Searching…" : "Find best strategy"}
        </Button>
      </div>

      {error && <Alert kind="error">{error}</Alert>}
      {applied && <Alert kind="success">Applied to your saved assumptions.</Alert>}

      {result && (
        <div className="table-scroll">
          <table className="proj-table compare-table">
            <thead>
              <tr>
                <th>Strategy</th>
                <th className="num">Roth ceiling</th>
                <th className="num">{goalMetricLabel(result.goal)}</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {result.candidates.map((c, i) => (
                <tr key={i} className={c.recommended ? "row-good" : ""}>
                  <td>
                    {strategyLabel(c.withdrawal_strategy)}
                    {c.recommended && " — recommended"}
                  </td>
                  <td className="num">
                    {c.roth_conversion_ceiling > 0
                      ? formatCurrency(c.roth_conversion_ceiling)
                      : "Off"}
                  </td>
                  <td className="num">{goalMetricValue(result.goal, c.summary)}</td>
                  <td>
                    {c.recommended && (
                      <Button variant="ghost" onClick={() => handleApply(c)} disabled={applying}>
                        {applying ? "Applying…" : "Apply to my plan"}
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

/**
 * Monte Carlo simulation (Phase 4, feature 6): runs the saved plan through
 * thousands of randomized-return trials, server-side, on demand. Unlike the
 * rest of the page this card doesn't block on the initial projection load —
 * running thousands of simulations is a slower, opt-in action — so it
 * manages its own request lifecycle entirely locally.
 */
function MonteCarloCard() {
  const [numSimulations, setNumSimulations] = useState(1000);
  const [volatility, setVolatility] = useState(12);
  const [result, setResult] = useState<MonteCarloResult | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleRun() {
    setError(null);
    setRunning(true);
    try {
      const r = await api.runMonteCarlo({ num_simulations: numSimulations, volatility });
      setResult(r);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to run simulation");
    } finally {
      setRunning(false);
    }
  }

  return (
    <Card title="Monte Carlo simulation" collapsible>
      <p className="muted">
        Runs your plan thousands of times with randomized investment returns each year to show the
        probability your money lasts, not just a single average-return path.
      </p>

      <div className="grid-2">
        <Field label="Number of simulations" htmlFor="mc-num-simulations">
          <Select
            id="mc-num-simulations"
            value={numSimulations}
            onChange={(e) => setNumSimulations(Number(e.target.value))}
          >
            <option value={1000}>1,000</option>
            <option value={5000}>5,000</option>
            <option value={10000}>10,000</option>
          </Select>
        </Field>
        <Field
          label="Return volatility (±%)"
          htmlFor="mc-volatility"
          hint="Standard deviation of each year's investment return, applied as a market-wide shock across every simulated year."
        >
          <TextInput
            id="mc-volatility"
            type="number"
            step="0.5"
            min="0"
            max="60"
            value={volatility}
            onChange={(e) => setVolatility(Number(e.target.value))}
          />
        </Field>
      </div>

      <div className="form-actions">
        <Button onClick={handleRun} disabled={running}>
          {running ? "Running…" : `Run ${numSimulations.toLocaleString()} simulations`}
        </Button>
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {result && (
        <>
          <div className="tile-grid">
            <div className={`tile ${result.success_rate >= 0.9 ? "tile-good" : "tile-warn"}`}>
              <span className="tile-label">Success rate</span>
              <span className="tile-value">{formatRate(result.success_rate)}</span>
              <span className="tile-sub muted">
                simulations where money lasted the full horizon
              </span>
            </div>
            <div className="tile">
              <span className="tile-label">Median ending balance</span>
              <span className="tile-value">{formatCurrency(result.median_ending_balance)}</span>
            </div>
            <div className="tile">
              <span className="tile-label">Best case</span>
              <span className="tile-value">{formatCurrency(result.best_case_ending_balance)}</span>
            </div>
            <div className="tile">
              <span className="tile-label">Worst case</span>
              <span className="tile-value">{formatCurrency(result.worst_case_ending_balance)}</span>
            </div>
          </div>
          <MonteCarloChart bands={result.percentile_bands} />
        </>
      )}
    </Card>
  );
}

/**
 * The current year's tax breakdown: parallel federal and state taxable-income
 * buildups, then a Federal · State · Combined comparison of tax owed and rates
 * (with a reserved property-tax row).
 */
function TaxBreakdownCard({
  tax,
  rothConversion,
  year,
  isCurrentYear,
}: {
  tax: YearTax;
  rothConversion: number;
  year: number;
  isCurrentYear: boolean;
}) {
  const gross =
    tax.ordinary_income +
    tax.qualified_dividends +
    tax.capital_gains +
    tax.social_security_benefits;
  const rate = (amount: number) => (gross > 0 ? amount / gross : 0);

  const stateGainsAsOrdinary = tax.qualified_dividends + tax.capital_gains;
  const stateTotal = tax.state_tax + tax.property_tax;
  const grandTotal = tax.total_tax + tax.property_tax;
  const hasProperty = tax.property_tax > 0;

  return (
    <Card title={`Tax breakdown · ${year}`} collapsible>
      <p className="muted">
        Estimated tax on this year's income, using {isCurrentYear ? "current" : "projected"}{" "}
        brackets. Federal treats qualified dividends and long-term gains at the preferential
        0/15/20% rates and taxes part of Social Security; most states (including California) instead
        tax gains and dividends as ordinary income and exempt Social Security, on their own brackets
        and standard deduction.
      </p>

      {/* Parallel taxable-income buildups */}
      <div className="tax-grid">
        <div className="tax-col">
          <h4>Federal taxable income</h4>
          <dl className="tax-lines">
            <TaxLine
              label="Ordinary income"
              value={tax.ordinary_income}
              hint={
                rothConversion > 0
                  ? `includes ${formatCurrency(rothConversion)} Roth conversion`
                  : undefined
              }
            />
            <TaxLine label="Qualified dividends" value={tax.qualified_dividends} />
            <TaxLine label="Capital gains" value={tax.capital_gains} />
            <TaxLine
              label="Social Security (taxable)"
              value={tax.taxable_social_security}
              hint={
                tax.social_security_benefits > 0
                  ? `of ${formatCurrency(tax.social_security_benefits)} received`
                  : undefined
              }
            />
            <TaxLine label="Standard deduction" value={-tax.standard_deduction} />
            <TaxLine label="Taxable income" value={tax.taxable_income} strong />
          </dl>
        </div>
        <div className="tax-col">
          <h4>State taxable income</h4>
          <dl className="tax-lines">
            <TaxLine label="Ordinary income" value={tax.ordinary_income} />
            <TaxLine
              label="Gains &amp; dividends"
              value={stateGainsAsOrdinary}
              hint="taxed as ordinary"
            />
            <TaxLine label="Social Security" value={0} hint="exempt" />
            <TaxLine label="State standard deduction" value={-tax.state_standard_deduction} />
            <TaxLine label="Taxable income" value={tax.state_taxable_income} strong />
          </dl>
        </div>
      </div>

      <p className="tax-magi-note muted">
        Modified AGI (MAGI) — the figure used for ACA subsidy and (in a later phase) Medicare IRMAA
        thresholds: <strong>{formatCurrency(tax.magi)}</strong>
      </p>

      {/* Federal · State · Combined comparison */}
      <div className="table-scroll">
        <table className="tax-compare">
          <thead>
            <tr>
              <th>Tax owed</th>
              <th className="num">Federal</th>
              <th className="num">State</th>
              <th className="num">Combined</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Income tax</td>
              <td className="num">{formatCurrency(tax.federal_tax)}</td>
              <td className="num">{formatCurrency(tax.state_tax)}</td>
              <td className="num">{formatCurrency(tax.total_tax)}</td>
            </tr>
            <tr className="tax-compare-reserved">
              <td>
                Property tax <span className="muted">· coming soon</span>
              </td>
              <td className="num muted">—</td>
              <td className="num">{hasProperty ? formatCurrency(tax.property_tax) : "—"}</td>
              <td className="num">{hasProperty ? formatCurrency(tax.property_tax) : "—"}</td>
            </tr>
            <tr className="tax-compare-total">
              <td>Total tax</td>
              <td className="num">{formatCurrency(tax.federal_tax)}</td>
              <td className="num">{formatCurrency(stateTotal)}</td>
              <td className="num">{formatCurrency(grandTotal)}</td>
            </tr>
            <tr>
              <td>Effective rate</td>
              <td className="num">{formatRate(rate(tax.federal_tax))}</td>
              <td className="num">{formatRate(rate(tax.state_tax))}</td>
              <td className="num">{formatRate(tax.effective_rate)}</td>
            </tr>
            <tr>
              <td>Marginal rate</td>
              <td className="num">{formatRate(tax.marginal_rate)}</td>
              <td className="num">{formatRate(tax.state_marginal_rate)}</td>
              <td className="num">{formatRate(tax.marginal_rate + tax.state_marginal_rate)}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </Card>
  );
}

/** Format an ISO date ("2026-04-15") as "Apr 15, 2026" without timezone drift. */
function formatDueDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  const date = new Date(Date.UTC(y, m - 1, d));
  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  });
}

/**
 * Estimated quarterly taxes (feature 7): the current year's projected tax split
 * into the four IRS Form 1040-ES installments, each with its due date.
 */
function EstimatedTaxesCard({ estimated }: { estimated: EstimatedTaxes }) {
  return (
    <Card title={`Estimated quarterly taxes · ${estimated.tax_year}`} collapsible>
      <p className="muted">{estimated.note}</p>
      {estimated.total <= 0 ? (
        <p className="muted center">
          No estimated payments required this year — your projected tax is $0.
        </p>
      ) : (
        <div className="estimate-grid">
          {estimated.payments.map((p) => (
            <div className="estimate-card" key={p.label}>
              <div className="estimate-head">
                <span className="estimate-label">{p.label}</span>
                <span className="muted estimate-period">{p.period}</span>
              </div>
              <span className="estimate-amt">{formatCurrency(p.amount)}</span>
              <span className="muted estimate-due">Due {formatDueDate(p.due_date)}</span>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}

/**
 * Multi-year tax report (Phase 2, feature 8): the full per-year federal/state
 * tax breakdown the "Tax breakdown" card only shows for the current year, plus
 * a CSV export a user can hand to an accountant.
 */
function TaxReportCard({
  annual,
  showWithdrawalOrder,
  onDownload,
  downloading,
  downloadError,
}: {
  annual: YearProjection[];
  showWithdrawalOrder: boolean;
  onDownload: () => void;
  downloading: boolean;
  downloadError: string | null;
}) {
  return (
    <Card title="Tax report" collapsible>
      <div className="report-head">
        <p className="muted report-head-copy">
          The full federal and state tax breakdown for every projected year — not just the current
          one — ready to export.
        </p>
        <Button variant="ghost" onClick={onDownload} disabled={downloading}>
          {downloading ? "Downloading…" : "Download CSV"}
        </Button>
      </div>
      {downloadError && <Alert kind="error">{downloadError}</Alert>}
      <div className="table-scroll">
        <table className="proj-table">
          <thead>
            <tr>
              <th>Year</th>
              <th>Age</th>
              {showWithdrawalOrder && <th>Order</th>}
              <th className="num">Ordinary income</th>
              <th className="num">Qual. div.</th>
              <th className="num">Cap. gains</th>
              <th className="num">Taxable SS</th>
              <th className="num">MAGI</th>
              <th className="num">Taxable income</th>
              <th className="num">Federal tax</th>
              <th className="num">State tax</th>
              <th className="num">Total tax</th>
              <th className="num">Eff. rate</th>
              <th className="num">Marg. rate</th>
            </tr>
          </thead>
          <tbody>
            {annual.map((y) => (
              <tr key={y.year}>
                <td>{y.year}</td>
                <td>{y.primary_age}</td>
                {showWithdrawalOrder && (
                  <td>
                    {y.withdrawal_order === "tax_deferred_first" ? "Tax-deferred first" : "Taxable first"}
                  </td>
                )}
                <td className="num">{formatCurrency(y.tax.ordinary_income)}</td>
                <td className="num">{formatCurrency(y.tax.qualified_dividends)}</td>
                <td className="num">{formatCurrency(y.tax.capital_gains)}</td>
                <td className="num">{formatCurrency(y.tax.taxable_social_security)}</td>
                <td className="num">{formatCurrency(y.tax.magi)}</td>
                <td className="num">{formatCurrency(y.tax.taxable_income)}</td>
                <td className="num">{formatCurrency(y.tax.federal_tax)}</td>
                <td className="num">{formatCurrency(y.tax.state_tax)}</td>
                <td className="num">{formatCurrency(y.tax.total_tax)}</td>
                <td className="num">{formatRate(y.tax.effective_rate)}</td>
                <td className="num">{formatRate(y.tax.marginal_rate)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

/**
 * ACA premium tax credit for the current year (Phase 3, feature 1): the MAGI →
 * FPL % → expected contribution buildup and the resulting subsidy, or a note
 * when the household is not eligible this year.
 */
function AcaSubsidyCard({ aca, year }: { aca: YearAca; year: number }) {
  const notEligibleReason =
    aca.magi > 0 && aca.fpl_percent < 100
      ? "Income is below 100% of the poverty line — Medicaid territory, so no marketplace premium tax credit."
      : "No premium tax credit this year — the household is at or past Medicare age (65), or income is too low to determine one.";

  return (
    <Card title={`ACA health insurance subsidy · ${year}`}>
      <p className="muted">
        The premium tax credit caps what you pay for the benchmark silver plan based on your income
        relative to the Federal Poverty Line. Withdrawals and Roth conversions raise your MAGI, which
        shrinks the credit — the tradeoff the planner makes visible.
      </p>
      {!aca.eligible ? (
        <p className="muted center">{notEligibleReason}</p>
      ) : (
        <dl className="tax-lines aca-lines">
          <TaxLine label="Modified AGI (MAGI)" value={aca.magi} />
          <TaxLine label="Federal Poverty Line" value={aca.federal_poverty_line} />
          <div className="tax-line">
            <dt>Income as % of poverty line</dt>
            <dd>{aca.fpl_percent.toFixed(0)}%</dd>
          </div>
          <div className="tax-line">
            <dt>
              Expected contribution
              <span className="muted tax-hint">
                {" "}
                {formatRate(aca.applicable_percentage)} of MAGI
              </span>
            </dt>
            <dd>{formatCurrency(aca.expected_contribution)}</dd>
          </div>
          <TaxLine label="Benchmark silver premium" value={aca.benchmark_premium} />
          <div className="tax-line tax-line-strong">
            <dt>Premium tax credit (subsidy)</dt>
            <dd>{formatCurrency(aca.subsidy)}</dd>
          </div>
        </dl>
      )}
    </Card>
  );
}

/**
 * Medicare IRMAA surcharge for the current year (Phase 3, feature 4): the
 * two-year MAGI lookback, the resulting Part B/D surcharge tier, and the
 * household total — or a note when the surcharge doesn't apply.
 */
function IrmaaCard({ irmaa, year }: { irmaa: YearIrmaa; year: number }) {
  return (
    <Card title={`Medicare IRMAA surcharge · ${year}`}>
      <p className="muted">
        IRMAA adds an income-based surcharge to the standard Medicare Part B and Part D premiums,
        based on household MAGI from two years prior ({irmaa.lookback_year}) — the same lookback the
        IRS/CMS use. It applies per enrolled household member, on top of the standard premiums shown
        above.
      </p>
      {!irmaa.has_lookback_data ? (
        <p className="muted center">
          No surcharge assumed — {irmaa.lookback_year} falls before the start of this plan, so its
          MAGI isn't modeled.
        </p>
      ) : !irmaa.applies ? (
        <p className="muted center">
          {irmaa.lookback_year} MAGI ({formatCurrency(irmaa.lookback_magi)}) was under the lowest
          IRMAA threshold — the household pays only the standard premiums.
        </p>
      ) : (
        <dl className="tax-lines aca-lines">
          <TaxLine label={`MAGI in ${irmaa.lookback_year}`} value={irmaa.lookback_magi} />
          <TaxLine label="Part B surcharge (per person/mo)" value={irmaa.part_b_surcharge_monthly} />
          <TaxLine label="Part D surcharge (per person/mo)" value={irmaa.part_d_surcharge_monthly} />
          <div className="tax-line">
            <dt>Enrolled household members</dt>
            <dd>{irmaa.enrolled_count}</dd>
          </div>
          <div className="tax-line tax-line-strong">
            <dt>Total IRMAA surcharge (annual)</dt>
            <dd>{formatCurrency(irmaa.total_surcharge)}</dd>
          </div>
        </dl>
      )}
    </Card>
  );
}

/** One labelled row in the tax-breakdown card. */
function TaxLine({
  label,
  value,
  hint,
  strong,
}: {
  label: string;
  value: number;
  hint?: string;
  strong?: boolean;
}) {
  return (
    <div className={`tax-line${strong ? " tax-line-strong" : ""}`}>
      <dt>
        {label}
        {hint && <span className="muted tax-hint"> {hint}</span>}
      </dt>
      <dd>{value < 0 ? formatSignedCurrency(value) : formatCurrency(value)}</dd>
    </div>
  );
}
