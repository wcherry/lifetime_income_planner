import { useEffect, useState } from "react";
import { api, ApiError } from "../api/client";
import type { Projection } from "../api/types";
import { Alert, Card } from "../components/ui";
import { NetWorthChart } from "../components/NetWorthChart";
import {
  categoryLabel,
  formatCurrency,
  formatPercent,
  formatSignedCurrency,
  hasWithdrawals,
  planOutlook,
} from "../data/projection";

export function ProjectionPage() {
  const [projection, setProjection] = useState<Projection | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [needsProfile, setNeedsProfile] = useState(false);

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

  const { summary, assumptions, annual, quarterly } = projection;
  const outlook = planOutlook(summary);
  const showWithdrawals = hasWithdrawals(projection);

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
      </div>

      {/* Net worth projection chart (feature 10) */}
      <Card title="Net worth over time">
        <p className="muted">
          Projected total account balance at the end of each year
          {summary.depletion_year != null && ", with the shortfall year marked in red"}. Hover a{" "}
          <span className="legend-key legend-key-in">$</span> life event or{" "}
          <span className="legend-key legend-key-flag">⚑</span> milestone for details.
        </p>
        <NetWorthChart annual={annual} depletionYear={summary.depletion_year} />
      </Card>

      {/* Quarterly withdrawal schedule (feature 9) */}
      <Card title={`Withdrawal schedule · ${projection.start_year}`}>
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
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* Annual projection table (feature 8) */}
      <Card title="Year-by-year projection">
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
                <th className="num">End</th>
                <th>Events</th>
              </tr>
            </thead>
            <tbody>
              {annual.map((y) => (
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
                  <td className="num">{formatCurrency(y.ending_balance)}</td>
                  <td>
                    {(y.life_events.length > 0 || y.milestones.length > 0) && (
                      <span className="event-badges">
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
              ))}
            </tbody>
          </table>
        </div>
      </Card>
    </div>
  );
}
