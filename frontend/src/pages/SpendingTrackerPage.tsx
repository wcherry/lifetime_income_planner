import { useEffect, useMemo, useState, type ChangeEvent, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type {
  CategoryMappingSuggestion,
  SpendingTrackerCategory,
  SpendingTrackerCategoryKind,
  SpendingTrackerImportResult,
  SpendingTrackerMonth,
  SpendingTrackerQuarterSummary,
  SpendingTrackerTransaction,
  SpendingTrackerYearSummary,
} from "../api/types";
import {
  Alert,
  Button,
  Card,
  Field,
  Modal,
  ModalFieldRow,
  Select,
  TextInput,
} from "../components/ui";
import { SpendingTrackerYearChart } from "../components/SpendingTrackerYearChart";
import { formatCurrencyCents, formatSignedCurrencyCents } from "../data/format";
import {
  computeCategorizedTotals,
  computeCategoryTotals,
  formatMonthLabel,
  monthsOfQuarter,
  quarterMonthCoverage,
} from "../data/spendingTracker";

const CATEGORY_KIND_OPTIONS: { value: SpendingTrackerCategoryKind; label: string }[] = [
  { value: "income", label: "Income" },
  { value: "expense", label: "Expense" },
  { value: "ignore", label: "Ignore" },
];

function categoryKindLabel(kind: SpendingTrackerCategoryKind): string {
  return CATEGORY_KIND_OPTIONS.find((o) => o.value === kind)?.label ?? kind;
}

/**
 * Spending Tracker: transaction-level CSV import + categorization — distinct
 * from the planned-budget "Spending" page (`SpendingPage.tsx`), which
 * manages budgeted assumptions consumed by the projection engine. This page
 * never touches that data.
 */
export function SpendingTrackerPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const scopeYearParam = searchParams.get("scopeYear");
  const scopeQuarterParam = searchParams.get("scopeQuarter");
  const scopeYear = scopeYearParam ? Number(scopeYearParam) : null;
  const scopeQuarter = scopeQuarterParam ? Number(scopeQuarterParam) : null;
  const inQuarterScope = scopeYear !== null && scopeQuarter !== null;

  const now = new Date();
  const [year, setYear] = useState(() => scopeYear ?? now.getFullYear());
  const [month, setMonth] = useState(() => {
    if (scopeYear !== null && scopeQuarter !== null) {
      try {
        return monthsOfQuarter(scopeQuarter)[0];
      } catch {
        // fall through to current month
      }
    }
    return now.getMonth() + 1;
  });

  const [categories, setCategories] = useState<SpendingTrackerCategory[]>([]);
  const [months, setMonths] = useState<SpendingTrackerMonth[]>([]);
  const [transactions, setTransactions] = useState<SpendingTrackerTransaction[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [csvContent, setCsvContent] = useState("");
  const [sourceFilename, setSourceFilename] = useState("");
  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] = useState<SpendingTrackerImportResult | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [pendingMappings, setPendingMappings] = useState<CategoryMappingSuggestion[]>([]);
  const [mappingSelections, setMappingSelections] = useState<Record<string, string>>({});
  const [applyingMappings, setApplyingMappings] = useState(false);

  const [showCategoriesModal, setShowCategoriesModal] = useState(false);
  const [showImportModal, setShowImportModal] = useState(false);
  const [showManualModal, setShowManualModal] = useState(false);
  const [manualDate, setManualDate] = useState("");
  const [manualDescription, setManualDescription] = useState("");
  const [manualAmount, setManualAmount] = useState("");
  const [manualCategoryId, setManualCategoryId] = useState("");
  const [addingTransaction, setAddingTransaction] = useState(false);
  const [manualError, setManualError] = useState<string | null>(null);

  const [addingCategory, setAddingCategory] = useState(false);
  const [newCategoryName, setNewCategoryName] = useState("");
  const [newCategoryKind, setNewCategoryKind] = useState<SpendingTrackerCategoryKind>("expense");
  const [categoryError, setCategoryError] = useState<string | null>(null);
  const [savingCategory, setSavingCategory] = useState(false);

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [bulkCategoryId, setBulkCategoryId] = useState("");
  const [bulkApplying, setBulkApplying] = useState(false);
  const [detailTxn, setDetailTxn] = useState<SpendingTrackerTransaction | null>(null);

  const [quarterSummary, setQuarterSummary] = useState<SpendingTrackerQuarterSummary | null>(null);
  const [yearSummary, setYearSummary] = useState<SpendingTrackerYearSummary | null>(null);

  async function refreshCategories() {
    setCategories(await api.listSpendingTrackerCategories());
  }

  async function refreshMonths() {
    setMonths(await api.listSpendingTrackerMonths());
  }

  async function refreshTransactions(forYear: number, forMonth: number) {
    setTransactions(await api.listSpendingTrackerTransactions(forYear, forMonth));
  }

  async function refreshQuarterSummary() {
    if (scopeYear === null || scopeQuarter === null) return;
    setQuarterSummary(await api.getSpendingTrackerQuarterSummary(scopeYear, scopeQuarter));
  }

  async function refreshYearSummary(forYear: number) {
    setYearSummary(await api.getSpendingTrackerYearSummary(forYear));
  }

  useEffect(() => {
    (async () => {
      try {
        await Promise.all([refreshCategories(), refreshMonths()]);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load Spending Tracker data");
      } finally {
        setLoading(false);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    setSelectedIds(new Set());
    setPendingMappings([]);
    setMappingSelections({});
    setDetailTxn(null);
    setManualError(null);
    setShowManualModal(false);
    setShowImportModal(false);
    refreshTransactions(year, month).catch((err) =>
      setError(err instanceof Error ? err.message : "Failed to load transactions"),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [year, month]);

  useEffect(() => {
    refreshQuarterSummary().catch((err) =>
      setError(err instanceof Error ? err.message : "Failed to load quarter summary"),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeYear, scopeQuarter]);

  useEffect(() => {
    refreshYearSummary(year).catch((err) =>
      setError(err instanceof Error ? err.message : "Failed to load year summary"),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [year]);

  const categoriesById = useMemo(() => new Map(categories.map((c) => [c.id, c])), [categories]);
  const categorizedTotals = useMemo(
    () => computeCategorizedTotals(transactions, categories),
    [transactions, categories],
  );
  const categoryTotals = useMemo(
    () => computeCategoryTotals(transactions, categories),
    [transactions, categories],
  );

  function handleCsvFileChange(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0] ?? null;
    setImportResult(null);
    setImportError(null);
    if (!file) {
      setCsvContent("");
      return;
    }
    setSourceFilename(file.name);
    file
      .text()
      .then(setCsvContent)
      .catch(() => setImportError("Could not read that file"));
  }

  async function handleImport(e: FormEvent) {
    e.preventDefault();
    if (!csvContent) return;
    setImportError(null);
    setImportResult(null);
    setImporting(true);
    try {
      const result = await api.importSpendingTransactions({
        year,
        month,
        csv_content: csvContent,
        source_filename: sourceFilename.trim() || null,
      });
      setImportResult(result);
      setPendingMappings(result.category_mappings);
      setMappingSelections(
        Object.fromEntries(
          result.category_mappings.map((m) => [m.label, m.suggested_category_id ?? ""]),
        ),
      );
      setCsvContent("");
      setSourceFilename("");
      const fileInput = document.getElementById("st_csv_file") as HTMLInputElement | null;
      if (fileInput) fileInput.value = "";
      await Promise.all([
        refreshCategories(),
        refreshMonths(),
        refreshTransactions(year, month),
        refreshQuarterSummary(),
        refreshYearSummary(year),
      ]);
    } catch (err) {
      setImportError(err instanceof Error ? err.message : "Failed to import transactions");
    } finally {
      setImporting(false);
    }
  }

  async function handleAddManualTransaction(e: FormEvent) {
    e.preventDefault();
    setManualError(null);
    setAddingTransaction(true);
    try {
      await api.createManualSpendingTrackerTransaction({
        year,
        month,
        transaction_date: manualDate,
        description: manualDescription.trim(),
        amount: Number(manualAmount),
        category_id: manualCategoryId || null,
      });
      setManualDate("");
      setManualDescription("");
      setManualAmount("");
      setManualCategoryId("");
      setShowManualModal(false);
      await Promise.all([
        refreshMonths(),
        refreshTransactions(year, month),
        refreshQuarterSummary(),
        refreshYearSummary(year),
      ]);
    } catch (err) {
      setManualError(err instanceof Error ? err.message : "Failed to add transaction");
    } finally {
      setAddingTransaction(false);
    }
  }

  async function handleApplyMappings() {
    setApplyingMappings(true);
    setError(null);
    try {
      // Sequential, not Promise.all — SQLite allows only one writer at a
      // time, and firing these concurrently just makes the later ones wait
      // on (or contend with) the earlier ones anyway.
      for (const m of pendingMappings) {
        if (!mappingSelections[m.label]) continue;
        await api.bulkCategorizeSpendingTrackerTransactions({
          transaction_ids: m.transaction_ids,
          category_id: mappingSelections[m.label],
        });
      }
      setPendingMappings([]);
      setMappingSelections({});
      await Promise.all([refreshTransactions(year, month), refreshYearSummary(year)]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to apply category mappings");
    } finally {
      setApplyingMappings(false);
    }
  }

  function dismissMappings() {
    setPendingMappings([]);
    setMappingSelections({});
  }

  async function handleAddCategory(e: FormEvent) {
    e.preventDefault();
    setCategoryError(null);
    setSavingCategory(true);
    try {
      await api.createSpendingTrackerCategory({
        name: newCategoryName.trim(),
        kind: newCategoryKind,
      });
      setNewCategoryName("");
      setNewCategoryKind("expense");
      setAddingCategory(false);
      await refreshCategories();
    } catch (err) {
      setCategoryError(err instanceof Error ? err.message : "Failed to create category");
    } finally {
      setSavingCategory(false);
    }
  }

  async function handleDeleteCategory(category: SpendingTrackerCategory) {
    if (
      !window.confirm(
        `Delete the category "${category.name}"? Its transactions become uncategorized.`,
      )
    )
      return;
    setError(null);
    try {
      await api.deleteSpendingTrackerCategory(category.id);
      await Promise.all([
        refreshCategories(),
        refreshTransactions(year, month),
        refreshYearSummary(year),
      ]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete category");
    }
  }

  async function handleSetTransactionCategory(txn: SpendingTrackerTransaction, categoryId: string) {
    const resolved = categoryId === "" ? null : categoryId;
    const previous = transactions;
    const category = resolved ? (categoriesById.get(resolved) ?? null) : null;
    setTransactions((cur) =>
      cur.map((t) =>
        t.id === txn.id
          ? {
              ...t,
              category_id: resolved,
              category_name: category?.name ?? null,
              category_kind: category?.kind ?? null,
            }
          : t,
      ),
    );
    try {
      await api.setSpendingTrackerTransactionCategory(txn.id, { category_id: resolved });
      await refreshYearSummary(year);
    } catch (err) {
      setTransactions(previous);
      setError(err instanceof Error ? err.message : "Failed to update category");
    }
  }

  function toggleSelected(id: string) {
    setSelectedIds((cur) => {
      const next = new Set(cur);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleSelectAll() {
    setSelectedIds((cur) =>
      cur.size === transactions.length ? new Set() : new Set(transactions.map((t) => t.id)),
    );
  }

  async function handleBulkApply() {
    if (selectedIds.size === 0) return;
    setBulkApplying(true);
    setError(null);
    try {
      await api.bulkCategorizeSpendingTrackerTransactions({
        transaction_ids: Array.from(selectedIds),
        category_id: bulkCategoryId === "" ? null : bulkCategoryId,
      });
      setSelectedIds(new Set());
      await Promise.all([refreshTransactions(year, month), refreshYearSummary(year)]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to apply category");
    } finally {
      setBulkApplying(false);
    }
  }

  function useTotalsInReview() {
    if (scopeYear === null || scopeQuarter === null || !quarterSummary) return;
    const params = new URLSearchParams({
      fillIncome: String(quarterSummary.income_total),
      fillSpending: String(quarterSummary.expense_total),
      fillYear: String(scopeYear),
      fillQuarter: String(scopeQuarter),
    });
    navigate(`/quarterly-review?${params.toString()}`);
  }

  function exitQuarterScope() {
    setSearchParams(new URLSearchParams(), { replace: true });
  }

  if (loading) return <p className="muted">Loading…</p>;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Spending Tracker</h1>
          <p className="muted">
            Import a month's bank or credit-card transactions from CSV, categorize them, and browse
            your spending history over time.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {inQuarterScope && scopeYear !== null && scopeQuarter !== null && (
        <Card title={`Quarter scope: ${scopeYear} Q${scopeQuarter}`}>
          <p className="muted">
            Reviewing {scopeYear} Q{scopeQuarter}. Import or categorize each month below, then pull
            the categorized totals back into your quarterly review.
          </p>
          <div className="month-picker">
            {quarterMonthCoverage(scopeYear, scopeQuarter, months).map((m) => (
              <button
                key={`${m.year}-${m.month}`}
                type="button"
                className={`month-chip${m.hasData ? " month-chip-has-data" : ""}${
                  m.year === year && m.month === month ? " month-chip-selected" : ""
                }`}
                onClick={() => {
                  setYear(m.year);
                  setMonth(m.month);
                }}
              >
                {formatMonthLabel(m.year, m.month).split(" ")[0].slice(0, 3)}
              </button>
            ))}
          </div>
          {quarterSummary && (
            <p className="muted">
              Categorized so far: income {formatCurrencyCents(quarterSummary.income_total)} ·
              expenses {formatCurrencyCents(quarterSummary.expense_total)}
            </p>
          )}
          <div className="form-actions">
            <Button onClick={useTotalsInReview} disabled={!quarterSummary}>
              Use these totals in Review
            </Button>
            <Button variant="ghost" onClick={exitQuarterScope}>
              Exit quarter scope
            </Button>
          </div>
        </Card>
      )}

      <Card title={`Expenses — ${year}`}>
        {yearSummary && yearSummary.expense_total > 0 ? (
          <SpendingTrackerYearChart year={year} categories={yearSummary.expense_categories} />
        ) : (
          <p className="muted center">No categorized expenses for {year} yet.</p>
        )}
      </Card>

      <Card title="Select month">
        <div className="grid-3">
          <Field label="Year" htmlFor="st_year">
            <TextInput
              id="st_year"
              type="number"
              value={year}
              onChange={(e) => setYear(Number(e.target.value))}
              required
            />
          </Field>
          <Field label="Month" htmlFor="st_month">
            <Select id="st_month" value={month} onChange={(e) => setMonth(Number(e.target.value))}>
              {Array.from({ length: 12 }, (_, i) => i + 1).map((m) => (
                <option key={m} value={m}>
                  {formatMonthLabel(year, m).split(" ")[0]}
                </option>
              ))}
            </Select>
          </Field>
          <div className="field">
            <label>&nbsp;</label>
            <Button variant="ghost" onClick={() => setShowImportModal(true)}>
              Import transaction
            </Button>
          </div>
        </div>
        {months.length > 0 && (
          <div className="month-picker">
            {months.map((m) => (
              <button
                key={`${m.year}-${m.month}`}
                type="button"
                className={`month-chip month-chip-has-data${
                  m.year === year && m.month === month ? " month-chip-selected" : ""
                }`}
                onClick={() => {
                  setYear(m.year);
                  setMonth(m.month);
                }}
              >
                {formatMonthLabel(m.year, m.month)}
              </button>
            ))}
          </div>
        )}
      </Card>

      {showImportModal && (
        <Modal title="Import transactions" onClose={() => setShowImportModal(false)}>
          <form onSubmit={handleImport} className="stack">
            <p className="muted">
              Importing into {formatMonthLabel(year, month)}. Re-importing the same file is a safe
              no-op — duplicate rows are detected and skipped.
            </p>
            <Field label="CSV file" htmlFor="st_csv_file">
              <input
                id="st_csv_file"
                type="file"
                accept=".csv,text/csv"
                onChange={handleCsvFileChange}
              />
            </Field>
            {importError && <Alert kind="error">{importError}</Alert>}
            <div className="form-actions">
              <Button type="submit" disabled={!csvContent || importing}>
                {importing ? "Importing…" : "Import transactions"}
              </Button>
              <Button type="button" variant="ghost" onClick={() => setShowImportModal(false)}>
                Done
              </Button>
            </div>
            {importResult && (
              <div className="import-result">
                <div className="import-result-counts">
                  <span className="import-result-count-imported">
                    {importResult.imported_count} transactions imported
                  </span>
                  <span className="import-result-count-duplicate">
                    {importResult.duplicate_count} duplicates skipped
                  </span>
                  <span className="import-result-count-skipped">
                    {importResult.skipped_rows.length} rows skipped
                  </span>
                </div>
                {importResult.skipped_rows.length > 0 && (
                  <details className="import-skipped">
                    <summary>Skipped rows ({importResult.skipped_rows.length})</summary>
                    <ul className="import-skipped-list">
                      {importResult.skipped_rows.map((row) => (
                        <li className="import-skipped-row" key={row.row_number}>
                          <span className="import-skipped-row-num">Row {row.row_number}:</span>{" "}
                          {row.reason}
                        </li>
                      ))}
                    </ul>
                  </details>
                )}
              </div>
            )}
          </form>
        </Modal>
      )}

      {pendingMappings.length > 0 && (
        <Card title="Map imported categories">
          <p className="muted">
            These CSV categories didn't exactly match one of your categories. Review the best-guess
            mapping below (or pick a different one), then apply — nothing is categorized until you
            do.
          </p>
          <div className="account-list">
            {pendingMappings.map((m) => (
              <div className="account-row" key={m.label}>
                <div className="account-main">
                  <span className="account-name">{m.label}</span>
                  <span className="account-meta muted">
                    {m.transaction_ids.length} txn{m.transaction_ids.length === 1 ? "" : "s"}
                    {m.suggested_category_name ? ` · best guess: ${m.suggested_category_name}` : ""}
                  </span>
                </div>
                <div className="account-actions">
                  <Select
                    aria-label={`Map "${m.label}" to category`}
                    value={mappingSelections[m.label] ?? ""}
                    onChange={(e) =>
                      setMappingSelections((cur) => ({ ...cur, [m.label]: e.target.value }))
                    }
                  >
                    <option value="">Leave uncategorized</option>
                    {categories.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                  </Select>
                </div>
              </div>
            ))}
          </div>
          <div className="form-actions">
            <Button onClick={handleApplyMappings} disabled={applyingMappings}>
              {applyingMappings ? "Applying…" : "Apply mappings"}
            </Button>
            <Button variant="ghost" onClick={dismissMappings}>
              Dismiss
            </Button>
          </div>
        </Card>
      )}

      {showCategoriesModal && (
        <Modal title="Categories" onClose={() => setShowCategoriesModal(false)}>
          {categoryError && <Alert kind="error">{categoryError}</Alert>}
          <div className="account-list">
            {categories.map((category) => (
              <div className="account-row" key={category.id}>
                <div className="account-main">
                  <span className="account-name">{category.name}</span>
                  <span className={`category-chip category-chip-${category.kind}`}>
                    {categoryKindLabel(category.kind)}
                  </span>
                </div>
                <div className="account-actions">
                  {category.is_predefined ? (
                    <span className="category-badge-predefined">Built-in</span>
                  ) : (
                    <Button variant="ghost" onClick={() => handleDeleteCategory(category)}>
                      Delete
                    </Button>
                  )}
                </div>
              </div>
            ))}
          </div>

          {addingCategory ? (
            <form onSubmit={handleAddCategory} className="stack">
              <div className="grid-3">
                <Field label="Name" htmlFor="new_category_name">
                  <TextInput
                    id="new_category_name"
                    value={newCategoryName}
                    onChange={(e) => setNewCategoryName(e.target.value)}
                    required
                  />
                </Field>
                <Field label="Kind" htmlFor="new_category_kind">
                  <Select
                    id="new_category_kind"
                    value={newCategoryKind}
                    onChange={(e) =>
                      setNewCategoryKind(e.target.value as SpendingTrackerCategoryKind)
                    }
                  >
                    {CATEGORY_KIND_OPTIONS.map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </Select>
                </Field>
              </div>
              <div className="form-actions">
                <Button type="submit" disabled={savingCategory}>
                  {savingCategory ? "Saving…" : "Save category"}
                </Button>
                <Button type="button" variant="ghost" onClick={() => setAddingCategory(false)}>
                  Cancel
                </Button>
              </div>
            </form>
          ) : (
            <div className="form-actions">
              <Button variant="ghost" onClick={() => setAddingCategory(true)}>
                Add category
              </Button>
            </div>
          )}
        </Modal>
      )}

      {showManualModal && (
        <Modal title="Add a transaction manually" onClose={() => setShowManualModal(false)}>
          <form onSubmit={handleAddManualTransaction} className="stack">
            <p className="muted">
              Adding to {formatMonthLabel(year, month)}. Useful for cash spending or anything a
              bank export missed.
            </p>
            <div className="grid-3">
              <Field label="Date" htmlFor="manual_date">
                <TextInput
                  id="manual_date"
                  type="date"
                  value={manualDate}
                  onChange={(e) => setManualDate(e.target.value)}
                  required
                />
              </Field>
              <Field label="Description" htmlFor="manual_description">
                <TextInput
                  id="manual_description"
                  value={manualDescription}
                  onChange={(e) => setManualDescription(e.target.value)}
                  required
                />
              </Field>
              <Field
                label="Amount"
                htmlFor="manual_amount"
                hint="Negative = expense, positive = income."
              >
                <TextInput
                  id="manual_amount"
                  type="number"
                  step="0.01"
                  value={manualAmount}
                  onChange={(e) => setManualAmount(e.target.value)}
                  required
                />
              </Field>
            </div>
            <Field label="Category (optional)" htmlFor="manual_category">
              <Select
                id="manual_category"
                value={manualCategoryId}
                onChange={(e) => setManualCategoryId(e.target.value)}
              >
                <option value="">Uncategorized</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </Select>
            </Field>
            {manualError && <Alert kind="error">{manualError}</Alert>}
            <div className="form-actions">
              <Button type="submit" disabled={addingTransaction}>
                {addingTransaction ? "Adding…" : "Add transaction"}
              </Button>
              <Button type="button" variant="ghost" onClick={() => setShowManualModal(false)}>
                Cancel
              </Button>
            </div>
          </form>
        </Modal>
      )}

      <Card title={`Transactions — ${formatMonthLabel(year, month)}`}>
        <div className="form-actions">
          <Button variant="ghost" onClick={() => setShowManualModal(true)}>
            Add transaction manually
          </Button>
          <Button variant="ghost" onClick={() => setShowCategoriesModal(true)}>
            Categories
          </Button>
        </div>
        {transactions.length === 0 ? (
          <p className="muted center">No transactions for this month yet.</p>
        ) : (
          <>
            <div className="tile-grid">
              <div className="tile tile-good">
                <span className="tile-label">Income</span>
                <span className="tile-value">
                  {formatCurrencyCents(categorizedTotals.incomeTotal)}
                </span>
              </div>
              <div className="tile tile-warn">
                <span className="tile-label">Expenses</span>
                <span className="tile-value">
                  {formatCurrencyCents(categorizedTotals.expenseTotal)}
                </span>
              </div>
              <div className="tile">
                <span className="tile-label">Net</span>
                <span className="tile-value">
                  {formatSignedCurrencyCents(
                    categorizedTotals.incomeTotal - categorizedTotals.expenseTotal,
                  )}
                </span>
              </div>
              <div className="tile">
                <span className="tile-label">Uncategorized</span>
                <span className="tile-value tile-value-sm">
                  {categorizedTotals.uncategorizedCount} txn
                  {categorizedTotals.uncategorizedCount === 1 ? "" : "s"}
                </span>
              </div>
            </div>

            {categoryTotals.length > 0 && (
              <div className="account-list">
                {categoryTotals.map((c) => (
                  <div className="account-row" key={c.categoryId}>
                    <div className="account-main">
                      <span className="account-name">{c.categoryName}</span>
                      <span className={`category-chip category-chip-${c.kind}`}>
                        {categoryKindLabel(c.kind)}
                      </span>
                    </div>
                    <div className="account-figures">
                      <span className="account-balance">{formatCurrencyCents(c.total)}</span>
                      <span className="account-meta muted">
                        {c.count} txn{c.count === 1 ? "" : "s"}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            )}

            {selectedIds.size > 0 && (
              <div className="bulk-action-bar">
                <span className="bulk-action-bar-count">{selectedIds.size} selected</span>
                <Select
                  aria-label="Category to apply to selected transactions"
                  value={bulkCategoryId}
                  onChange={(e) => setBulkCategoryId(e.target.value)}
                >
                  <option value="">Uncategorized</option>
                  {categories.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </Select>
                <Button onClick={handleBulkApply} disabled={bulkApplying}>
                  {bulkApplying ? "Applying…" : "Apply category to selected"}
                </Button>
                <Button variant="ghost" onClick={() => setSelectedIds(new Set())}>
                  Clear selection
                </Button>
              </div>
            )}
            <div className="table-scroll">
              <table className="proj-table">
                <thead>
                  <tr>
                    <th className="checkbox-cell">
                      <input
                        type="checkbox"
                        aria-label="Select all transactions"
                        checked={selectedIds.size === transactions.length}
                        onChange={toggleSelectAll}
                      />
                    </th>
                    <th>Date</th>
                    <th>Description</th>
                    <th>Amount</th>
                    <th>Category</th>
                  </tr>
                </thead>
                <tbody>
                  {transactions.map((txn) => (
                    <tr
                      key={txn.id}
                      className={`row-clickable${selectedIds.has(txn.id) ? " row-selected" : ""}`}
                      onClick={() => setDetailTxn(txn)}
                    >
                      <td className="checkbox-cell" onClick={(e) => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          aria-label={`Select ${txn.description}`}
                          checked={selectedIds.has(txn.id)}
                          onChange={() => toggleSelected(txn.id)}
                        />
                      </td>
                      <td>{txn.transaction_date}</td>
                      <td className="txn-description-cell">{txn.description}</td>
                      <td className={`num ${txn.amount >= 0 ? "amount-income" : "amount-expense"}`}>
                        {formatCurrencyCents(txn.amount)}
                      </td>
                      <td onClick={(e) => e.stopPropagation()}>
                        <Select
                          aria-label={`Category for ${txn.description}`}
                          value={txn.category_id ?? ""}
                          onChange={(e) => handleSetTransactionCategory(txn, e.target.value)}
                        >
                          <option value="">Uncategorized</option>
                          {categories.map((c) => (
                            <option key={c.id} value={c.id}>
                              {c.name}
                            </option>
                          ))}
                        </Select>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </Card>

      {detailTxn && (
        <Modal title="Imported values" onClose={() => setDetailTxn(null)}>
          <p className="muted">
            Exactly what was in the imported file for this row, before any parsing.
          </p>
          <div className="modal-field-list">
            {Object.entries(detailTxn.raw_row).map(([header, value]) => (
              <ModalFieldRow key={header} label={header} value={value || "—"} />
            ))}
          </div>
        </Modal>
      )}
    </div>
  );
}
