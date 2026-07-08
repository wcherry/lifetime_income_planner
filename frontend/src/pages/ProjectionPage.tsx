import { useEffect, useState } from "react";
import { api, ApiError } from "../api/client";
import type { EstimatedTaxes, Projection, YearAca, YearProjection, YearTax } from "../api/types";
import { Alert, Button, Card } from "../components/ui";
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
