import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { TaxDocument, TaxDocumentYearSummary, TaxFormType } from "../api/types";
import { Alert, Button, Card, Field, Select, TextInput } from "../components/ui";
import { formatCurrency } from "../data/format";

const TAX_FORM_TYPE_OPTIONS: { value: TaxFormType; label: string }[] = [
  { value: "1099-div", label: "1099-DIV (dividends)" },
  { value: "1099-int", label: "1099-INT (interest)" },
  { value: "1099-r", label: "1099-R (retirement distributions)" },
  { value: "w2", label: "W-2 (wages)" },
  { value: "ssa-1099", label: "SSA-1099 (Social Security)" },
];

function taxFormTypeLabel(t: TaxFormType): string {
  return TAX_FORM_TYPE_OPTIONS.find((o) => o.value === t)?.label ?? t;
}

function fieldLabel(key: string): string {
  return key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export function TaxDocumentsPage() {
  const currentYear = new Date().getFullYear();
  const [documents, setDocuments] = useState<TaxDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  const [taxYear, setTaxYear] = useState(currentYear);
  const [formType, setFormType] = useState<TaxFormType>("1099-div");
  const [csvContent, setCsvContent] = useState("ordinary_dividends,0.00\nqualified_dividends,0.00");
  const [sourceFilename, setSourceFilename] = useState("");

  const [summaryYear, setSummaryYear] = useState(currentYear);
  const [summary, setSummary] = useState<TaxDocumentYearSummary | null>(null);
  const [summaryError, setSummaryError] = useState<string | null>(null);

  async function refresh() {
    try {
      setDocuments(await api.listTaxDocuments());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load tax documents");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function loadSummary(year: number) {
    setSummaryError(null);
    try {
      setSummary(await api.getTaxDocumentYearSummary(year));
    } catch (err) {
      setSummaryError(err instanceof Error ? err.message : "Failed to load summary");
    }
  }

  useEffect(() => {
    loadSummary(summaryYear);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [summaryYear]);

  async function handleImport(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setImporting(true);
    try {
      await api.importTaxDocument({
        tax_year: taxYear,
        form_type: formType,
        csv_content: csvContent,
        source_filename: sourceFilename.trim() || null,
      });
      setSourceFilename("");
      await refresh();
      await loadSummary(summaryYear);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import");
    } finally {
      setImporting(false);
    }
  }

  async function handleDelete(doc: TaxDocument) {
    if (!window.confirm(`Delete this imported ${taxFormTypeLabel(doc.form_type)}?`)) return;
    setError(null);
    try {
      await api.deleteTaxDocument(doc.id);
      await refresh();
      await loadSummary(summaryYear);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  if (loading) return <p className="muted">Loading tax documents…</p>;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Tax documents</h1>
          <p className="muted">
            Import 1099s, W-2s, and SSA-1099s to compare actual reported income against the tax
            projection.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      <Card title="Import a tax document">
        <form onSubmit={handleImport} className="account-form">
          <div className="grid-3">
            <Field label="Tax year" htmlFor="tax_year">
              <TextInput
                id="tax_year"
                type="number"
                value={taxYear}
                onChange={(e) => setTaxYear(Number(e.target.value))}
                required
              />
            </Field>
            <Field label="Form type" htmlFor="form_type">
              <Select
                id="form_type"
                value={formType}
                onChange={(e) => setFormType(e.target.value as TaxFormType)}
              >
                {TAX_FORM_TYPE_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Source filename (optional)" htmlFor="source_filename">
              <TextInput
                id="source_filename"
                value={sourceFilename}
                onChange={(e) => setSourceFilename(e.target.value)}
                placeholder="e.g. fidelity-2026-1099div.pdf"
              />
            </Field>
          </div>

          <Field
            label="Box amounts (CSV)"
            htmlFor="csv_content"
            hint="One field,amount per line — a header row is fine and gets skipped."
          >
            <textarea
              id="csv_content"
              className="input"
              rows={6}
              value={csvContent}
              onChange={(e) => setCsvContent(e.target.value)}
              required
            />
          </Field>

          <div className="form-actions">
            <Button type="submit" disabled={importing}>
              {importing ? "Importing…" : "Import"}
            </Button>
          </div>
        </form>
      </Card>

      <Card title="Year summary">
        <div className="grid-3">
          <Field label="Year" htmlFor="summary_year">
            <TextInput
              id="summary_year"
              type="number"
              value={summaryYear}
              onChange={(e) => setSummaryYear(Number(e.target.value))}
            />
          </Field>
        </div>
        {summaryError && <Alert kind="error">{summaryError}</Alert>}
        {summary && (
          <div className="stack">
            <p className="muted">
              {summary.document_count} document{summary.document_count === 1 ? "" : "s"} ·{" "}
              <strong>{formatCurrency(summary.grand_total)}</strong> total
            </p>
            {Object.keys(summary.totals_by_field).length > 0 && (
              <div className="account-list">
                {Object.entries(summary.totals_by_field).map(([field, amount]) => (
                  <div className="account-row" key={field}>
                    <div className="account-main">
                      <span className="account-name">{fieldLabel(field)}</span>
                    </div>
                    <div className="account-figures">
                      <span className="account-balance">{formatCurrency(amount)}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </Card>

      {documents.length === 0 ? (
        <Card>
          <p className="muted center">No tax documents imported yet.</p>
        </Card>
      ) : (
        <div className="account-list">
          {documents.map((doc) => (
            <div className="account-row" key={doc.id}>
              <div className="account-main">
                <span className="account-name">
                  {doc.tax_year} · {taxFormTypeLabel(doc.form_type)}
                </span>
                <span className="account-meta muted">
                  {Object.keys(doc.box_data).length} field
                  {Object.keys(doc.box_data).length === 1 ? "" : "s"}
                  {doc.source_filename ? ` · ${doc.source_filename}` : ""} · imported{" "}
                  {new Date(doc.imported_at).toLocaleDateString()}
                </span>
              </div>
              <div className="account-figures">
                <span className="account-balance">{formatCurrency(doc.total)}</span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => handleDelete(doc)}>
                  Delete
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
