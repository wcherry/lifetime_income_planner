import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { DueQuarterlyReview, QuarterlyReview, QuarterlyReviewOverview } from "../api/types";
import { Alert, Button, Card, Field, TextInput } from "../components/ui";
import { categoryLabel } from "../data/accounts";
import { formatCurrency, formatSignedCurrency } from "../data/format";
import {
  allBalancesEntered,
  previewReconciliation,
  toActualBalances,
} from "../data/quarterlyReview";

/**
 * Quarterly review (roadmap Phase 5): reconcile each quarter's actual
 * income/spending/tax and account balances against what the plan projected,
 * and let the user complete a review to feed those actuals back into their
 * live account balances.
 */
export function QuarterlyReviewPage() {
  const [overview, setOverview] = useState<QuarterlyReviewOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [selectedPeriod, setSelectedPeriod] = useState<DueQuarterlyReview | null>(null);
  const [actualIncome, setActualIncome] = useState("");
  const [actualSpending, setActualSpending] = useState("");
  const [actualTax, setActualTax] = useState("");
  const [balances, setBalances] = useState<Record<string, string>>({});
  const [notes, setNotes] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [expandedHistoryId, setExpandedHistoryId] = useState<string | null>(null);

  async function refresh() {
    try {
      setOverview(await api.getQuarterlyReviews());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load quarterly reviews");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  function selectPeriod(period: DueQuarterlyReview) {
    setError(null);
    setNotice(null);
    setSelectedPeriod(period);
    setActualIncome(String(period.planned_income));
    setActualSpending(String(period.planned_spending));
    setActualTax(String(period.planned_tax));
    const seeded: Record<string, string> = {};
    for (const account of period.accounts)
      seeded[account.account_id] = String(account.current_balance);
    setBalances(seeded);
    setNotes("");
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!selectedPeriod) return;
    if (
      !window.confirm(
        `Complete the review for ${selectedPeriod.label}? This will overwrite your account balances ` +
          `with the ending balances entered below.`,
      )
    )
      return;
    setError(null);
    setNotice(null);
    setSubmitting(true);
    try {
      await api.completeQuarterlyReview(selectedPeriod.year, selectedPeriod.quarter, {
        actual_income: Number(actualIncome),
        actual_spending: Number(actualSpending),
        actual_tax: Number(actualTax),
        actual_balances: toActualBalances(balances),
        notes: notes.trim() === "" ? null : notes.trim(),
      });
      setNotice(`Completed the review for ${selectedPeriod.label}.`);
      setSelectedPeriod(null);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to complete review");
    } finally {
      setSubmitting(false);
    }
  }

  function toggleHistory(review: QuarterlyReview) {
    setExpandedHistoryId((cur) => (cur === review.id ? null : review.id));
  }

  if (loading) return <p className="muted">Loading…</p>;
  if (!overview) return null;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Quarterly review</h1>
          <p className="muted">
            Enter your actual income, spending, tax, and account balances each quarter to reconcile
            them against your plan and keep your projections grounded in reality.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}
      {notice && <Alert kind="success">{notice}</Alert>}

      <Card title="Needs review">
        {overview.due.length === 0 ? (
          <Alert kind="success">You&rsquo;re all caught up.</Alert>
        ) : (
          <div className="account-list">
            {overview.due.map((period) => (
              <DueReviewRow
                key={`${period.year}-${period.quarter}`}
                period={period}
                onSelect={selectPeriod}
              />
            ))}
          </div>
        )}
      </Card>

      {selectedPeriod && (
        <ReviewForm
          period={selectedPeriod}
          actualIncome={actualIncome}
          actualSpending={actualSpending}
          actualTax={actualTax}
          balances={balances}
          notes={notes}
          submitting={submitting}
          onActualIncomeChange={setActualIncome}
          onActualSpendingChange={setActualSpending}
          onActualTaxChange={setActualTax}
          onBalanceChange={(id, value) => setBalances((cur) => ({ ...cur, [id]: value }))}
          onNotesChange={setNotes}
          onSubmit={handleSubmit}
        />
      )}

      <Card title="History">
        {overview.history.length === 0 ? (
          <p className="muted center">No completed reviews yet.</p>
        ) : (
          <div className="account-list">
            {overview.history.map((review) => (
              <HistoryRow
                key={review.id}
                review={review}
                expanded={expandedHistoryId === review.id}
                onToggle={() => toggleHistory(review)}
              />
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}

function DueReviewRow({
  period,
  onSelect,
}: {
  period: DueQuarterlyReview;
  onSelect: (period: DueQuarterlyReview) => void;
}) {
  return (
    <div className="account-row">
      <div className="account-main">
        <span className="account-name">{period.label}</span>
        <span className="account-meta muted">
          Planned: income {formatCurrency(period.planned_income)} · spending{" "}
          {formatCurrency(period.planned_spending)} · tax {formatCurrency(period.planned_tax)} ·
          withdrawal {formatCurrency(period.planned_withdrawal)}
        </span>
      </div>
      <div className="account-actions">
        <Button onClick={() => onSelect(period)}>Review</Button>
      </div>
    </div>
  );
}

function ReviewForm({
  period,
  actualIncome,
  actualSpending,
  actualTax,
  balances,
  notes,
  submitting,
  onActualIncomeChange,
  onActualSpendingChange,
  onActualTaxChange,
  onBalanceChange,
  onNotesChange,
  onSubmit,
}: {
  period: DueQuarterlyReview;
  actualIncome: string;
  actualSpending: string;
  actualTax: string;
  balances: Record<string, string>;
  notes: string;
  submitting: boolean;
  onActualIncomeChange: (value: string) => void;
  onActualSpendingChange: (value: string) => void;
  onActualTaxChange: (value: string) => void;
  onBalanceChange: (accountId: string, value: string) => void;
  onNotesChange: (value: string) => void;
  onSubmit: (e: FormEvent) => void;
}) {
  const startingBalances = period.accounts.map((account) => account.current_balance);
  const enteredEndingBalances = period.accounts.map((account) => {
    const parsed = Number(balances[account.account_id]);
    return Number.isFinite(parsed) ? parsed : 0;
  });
  const numericIncome = Number(actualIncome);
  const numericSpending = Number(actualSpending);
  const numericTax = Number(actualTax);
  const preview = previewReconciliation(
    startingBalances,
    enteredEndingBalances,
    Number.isFinite(numericIncome) ? numericIncome : 0,
    Number.isFinite(numericSpending) ? numericSpending : 0,
    Number.isFinite(numericTax) ? numericTax : 0,
  );
  const canSubmit =
    allBalancesEntered(
      period.accounts.map((account) => account.account_id),
      balances,
    ) && !submitting;

  return (
    <Card title={`Review ${period.label}`}>
      <form onSubmit={onSubmit} className="stack">
        <p className="muted">
          Planned this quarter: income {formatCurrency(period.planned_income)}, spending{" "}
          {formatCurrency(period.planned_spending)}, tax {formatCurrency(period.planned_tax)},
          withdrawal {formatCurrency(period.planned_withdrawal)}.
        </p>

        <Field label="Actual income" htmlFor="qr-income">
          <TextInput
            id="qr-income"
            type="number"
            min={0}
            value={actualIncome}
            onChange={(e) => onActualIncomeChange(e.target.value)}
            required
          />
        </Field>
        <Field label="Actual spending" htmlFor="qr-spending">
          <TextInput
            id="qr-spending"
            type="number"
            min={0}
            value={actualSpending}
            onChange={(e) => onActualSpendingChange(e.target.value)}
            required
          />
        </Field>
        <Field label="Actual tax paid" htmlFor="qr-tax">
          <TextInput
            id="qr-tax"
            type="number"
            min={0}
            value={actualTax}
            onChange={(e) => onActualTaxChange(e.target.value)}
            required
          />
        </Field>

        <div className="account-list">
          {period.accounts.map((account) => (
            <div className="account-row" key={account.account_id}>
              <div className="account-main">
                <span className="account-name">{account.account_name}</span>
                <span className="account-meta muted">
                  {categoryLabel(account.category)} · was {formatCurrency(account.current_balance)}
                </span>
              </div>
              <div className="account-actions">
                <TextInput
                  type="number"
                  min={0}
                  aria-label={`Actual ending balance for ${account.account_name}`}
                  value={balances[account.account_id] ?? ""}
                  onChange={(e) => onBalanceChange(account.account_id, e.target.value)}
                  required
                />
              </div>
            </div>
          ))}
        </div>

        <Field label="Notes" htmlFor="qr-notes">
          <textarea
            id="qr-notes"
            className="input"
            rows={3}
            value={notes}
            onChange={(e) => onNotesChange(e.target.value)}
          />
        </Field>

        <p className="muted">
          Starting {formatCurrency(preview.total_starting_balance)} → ending{" "}
          {formatCurrency(preview.total_ending_balance)} (
          {formatSignedCurrency(preview.net_balance_change)}). Net cash flow{" "}
          {formatSignedCurrency(preview.net_cash_flow)}. Implied investment{" "}
          {preview.implied_investment_gain >= 0 ? "gain" : "loss"}:{" "}
          {formatSignedCurrency(preview.implied_investment_gain)}.
        </p>

        <p className="muted">
          Completing this review will update your account balances to the values above.
        </p>

        <div className="form-actions">
          <Button type="submit" disabled={!canSubmit}>
            {submitting ? "Completing…" : "Complete review"}
          </Button>
        </div>
      </form>
    </Card>
  );
}

function HistoryRow({
  review,
  expanded,
  onToggle,
}: {
  review: QuarterlyReview;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <>
      <div className="account-row">
        <div className="account-main">
          <span className="account-name">{review.label}</span>
          <span className="account-meta muted">
            Income {formatSignedCurrency(review.income_variance)} · spending{" "}
            {formatSignedCurrency(review.spending_variance)} · tax{" "}
            {formatSignedCurrency(review.tax_variance)} vs. plan · implied investment{" "}
            {formatSignedCurrency(review.reconciliation.implied_investment_gain)}
          </span>
        </div>
        <div className="account-actions">
          <Button variant="ghost" onClick={onToggle}>
            {expanded ? "Hide" : "Details"}
          </Button>
        </div>
      </div>
      {expanded && <HistoryDetail review={review} />}
    </>
  );
}

function HistoryDetail({ review }: { review: QuarterlyReview }) {
  return (
    <div className="card-body">
      <p className="muted">
        Income: planned {formatCurrency(review.planned_income)}, actual{" "}
        {formatCurrency(review.actual_income)} ({formatSignedCurrency(review.income_variance)}).
      </p>
      <p className="muted">
        Spending: planned {formatCurrency(review.planned_spending)}, actual{" "}
        {formatCurrency(review.actual_spending)} ({formatSignedCurrency(review.spending_variance)}).
      </p>
      <p className="muted">
        Tax: planned {formatCurrency(review.planned_tax)}, actual{" "}
        {formatCurrency(review.actual_tax)} ({formatSignedCurrency(review.tax_variance)}).
      </p>
      <p className="muted">Planned withdrawal: {formatCurrency(review.planned_withdrawal)}.</p>

      <div className="table-scroll">
        <table className="proj-table">
          <thead>
            <tr>
              <th>Account</th>
              <th>Category</th>
              <th>Starting</th>
              <th>Ending</th>
              <th>Change</th>
            </tr>
          </thead>
          <tbody>
            {review.balances.map((balance) => (
              <tr key={balance.account_id}>
                <td>{balance.account_name}</td>
                <td>{categoryLabel(balance.category)}</td>
                <td className="num">{formatCurrency(balance.starting_balance)}</td>
                <td className="num">{formatCurrency(balance.ending_balance)}</td>
                <td className="num">
                  {formatSignedCurrency(balance.ending_balance - balance.starting_balance)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="muted">
        Starting {formatCurrency(review.reconciliation.total_starting_balance)} → ending{" "}
        {formatCurrency(review.reconciliation.total_ending_balance)} (
        {formatSignedCurrency(review.reconciliation.net_balance_change)}). Net cash flow{" "}
        {formatSignedCurrency(review.reconciliation.net_cash_flow)}. Implied investment gain/loss:{" "}
        {formatSignedCurrency(review.reconciliation.implied_investment_gain)}.
      </p>

      {review.notes && <p className="muted">Notes: {review.notes}</p>}
    </div>
  );
}
