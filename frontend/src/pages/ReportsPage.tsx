import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import { Alert, Button, Card } from "../components/ui";

/**
 * A single download action with its own loading/error state, so multiple
 * report buttons on this page don't share one flag.
 */
function DownloadButton({
  label,
  onDownload,
}: {
  label: string;
  onDownload: () => Promise<void>;
}) {
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleClick() {
    setError(null);
    setDownloading(true);
    try {
      await onDownload();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Download failed");
    } finally {
      setDownloading(false);
    }
  }

  return (
    <div className="report-download">
      <Button variant="ghost" onClick={handleClick} disabled={downloading}>
        {downloading ? "Downloading…" : label}
      </Button>
      {error && <Alert kind="error">{error}</Alert>}
    </div>
  );
}

/**
 * Report generation and CSV/PDF export (roadmap Phase 5, features 6 & 7): a
 * central place to download the plan's generated reports. The tax summary
 * report (Phase 2, feature 8) already lives on the Plan page as an on-screen
 * table alongside its download button — linked from here rather than
 * duplicated.
 */
export function ReportsPage() {
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Reports</h1>
          <p className="muted">Download your plan as a document — for a spreadsheet, an accountant, or your own records.</p>
        </div>
      </div>

      <Card title="Annual plan report">
        <p className="muted">
          One row per projected year: net worth, income, spending, withdrawals, RMDs, Roth
          conversions, and taxes — a full multi-year summary of the plan.
        </p>
        <div className="report-download-row">
          <DownloadButton label="Download CSV" onDownload={api.downloadAnnualPlanCsv} />
          <DownloadButton label="Download PDF" onDownload={api.downloadAnnualPlanPdf} />
        </div>
      </Card>

      <Card title="Quarterly schedule">
        <p className="muted">
          The near-term quarterly withdrawal schedule: income, spending, estimated tax, and which
          accounts to draw from each quarter.
        </p>
        <div className="report-download-row">
          <DownloadButton label="Download CSV" onDownload={api.downloadQuarterlyScheduleCsv} />
        </div>
      </Card>

      <Card title="Asset allocation">
        <p className="muted">
          A snapshot of your current accounts — balance, category, and stock/bond/cash
          allocation.
        </p>
        <div className="report-download-row">
          <DownloadButton label="Download CSV" onDownload={api.downloadAssetAllocationCsv} />
        </div>
      </Card>

      <Card title="Tax summary">
        <p className="muted">
          The full federal and state tax breakdown for every projected year lives on the{" "}
          <Link to="/projection">Plan page</Link>, alongside its own CSV download.
        </p>
      </Card>
    </div>
  );
}
