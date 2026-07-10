import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { Profile, SocialSecurityEstimate, SsEstimateOwner } from "../api/types";
import { Alert, Button, Card, Field, Select, TextInput } from "../components/ui";
import { formatCurrency } from "../data/format";

const CLAIMING_AGES = [62, 67, 70] as const;

function ownerLabel(o: SsEstimateOwner): string {
  return o === "self" ? "Self" : "Spouse";
}

/** Add `years` to an ISO date string, returning an ISO date string. */
function addYears(isoDate: string, years: number): string {
  const d = new Date(isoDate);
  d.setFullYear(d.getFullYear() + years);
  return d.toISOString().slice(0, 10);
}

export function SocialSecurityPage() {
  const [estimates, setEstimates] = useState<SocialSecurityEstimate[]>([]);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  const [owner, setOwner] = useState<SsEstimateOwner>("self");
  const [statementDate, setStatementDate] = useState("");
  const [at62, setAt62] = useState("");
  const [at67, setAt67] = useState("");
  const [at70, setAt70] = useState("");

  async function refresh() {
    try {
      const [estimateList, prof] = await Promise.all([
        api.listSocialSecurityEstimates(),
        api.getProfile().catch(() => null),
      ]);
      setEstimates(estimateList);
      setProfile(prof);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load Social Security estimates");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleImport(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setImporting(true);
    try {
      await api.importSocialSecurityEstimate({
        owner,
        statement_date: statementDate,
        estimate_at_62: at62 ? Number(at62) : null,
        estimate_at_67: at67 ? Number(at67) : null,
        estimate_at_70: at70 ? Number(at70) : null,
      });
      setAt62("");
      setAt67("");
      setAt70("");
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import");
    } finally {
      setImporting(false);
    }
  }

  async function handleDelete(estimate: SocialSecurityEstimate) {
    if (!window.confirm(`Delete this ${ownerLabel(estimate.owner)} estimate?`)) return;
    setError(null);
    try {
      await api.deleteSocialSecurityEstimate(estimate.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  function birthDateFor(o: SsEstimateOwner): string | null {
    if (!profile) return null;
    return o === "self" ? profile.date_of_birth : profile.spouse_date_of_birth;
  }

  async function handleUseEstimate(estimate: SocialSecurityEstimate, age: number, amount: number) {
    const dob = birthDateFor(estimate.owner);
    if (!dob) {
      setError(
        `Add ${estimate.owner === "self" ? "your" : "your spouse's"} date of birth on the Profile page first.`,
      );
      return;
    }
    setError(null);
    setStatus(null);
    try {
      await api.createIncome({
        name: `Social Security (claimed at ${age})`,
        income_type: "social_security",
        owner: estimate.owner,
        amount,
        frequency: "monthly",
        start_date: addYears(dob, age),
        growth_rate: 0,
        cola: true,
        taxability: "partially_taxable",
      });
      setStatus(`Created a Social Security income source starting at age ${age}.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create income source");
    }
  }

  if (loading) return <p className="muted">Loading Social Security estimates…</p>;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Social Security</h1>
          <p className="muted">
            Import your SSA statement's claiming-age estimates, then generate an income source from
            whichever age you plan to claim.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}
      {status && <Alert kind="success">{status}</Alert>}

      <Card title="Import a statement">
        <form onSubmit={handleImport} className="account-form">
          <div className="grid-3">
            <Field label="Whose statement" htmlFor="ss_owner">
              <Select
                id="ss_owner"
                value={owner}
                onChange={(e) => setOwner(e.target.value as SsEstimateOwner)}
              >
                <option value="self">Self</option>
                <option value="spouse">Spouse</option>
              </Select>
            </Field>
            <Field label="Statement date" htmlFor="statement_date">
              <TextInput
                id="statement_date"
                type="date"
                value={statementDate}
                onChange={(e) => setStatementDate(e.target.value)}
                required
              />
            </Field>
          </div>

          <div className="grid-3">
            <Field label="Estimate at 62 ($/mo)" htmlFor="at62">
              <TextInput
                id="at62"
                type="number"
                min={0}
                step="0.01"
                value={at62}
                onChange={(e) => setAt62(e.target.value)}
              />
            </Field>
            <Field label="Estimate at 67 ($/mo)" htmlFor="at67">
              <TextInput
                id="at67"
                type="number"
                min={0}
                step="0.01"
                value={at67}
                onChange={(e) => setAt67(e.target.value)}
              />
            </Field>
            <Field label="Estimate at 70 ($/mo)" htmlFor="at70">
              <TextInput
                id="at70"
                type="number"
                min={0}
                step="0.01"
                value={at70}
                onChange={(e) => setAt70(e.target.value)}
              />
            </Field>
          </div>

          <div className="form-actions">
            <Button type="submit" disabled={importing}>
              {importing ? "Importing…" : "Import"}
            </Button>
          </div>
        </form>
      </Card>

      {estimates.length === 0 ? (
        <Card>
          <p className="muted center">No Social Security estimates imported yet.</p>
        </Card>
      ) : (
        <div className="account-list">
          {estimates.map((estimate) => (
            <div className="account-row" key={estimate.id}>
              <div className="account-main">
                <span className="account-name">
                  {ownerLabel(estimate.owner)} · statement {estimate.statement_date}
                </span>
                <span className="account-meta muted">
                  {CLAIMING_AGES.map((age) => {
                    const amount =
                      age === 62
                        ? estimate.estimate_at_62
                        : age === 67
                          ? estimate.estimate_at_67
                          : estimate.estimate_at_70;
                    return amount != null ? `Age ${age}: ${formatCurrency(amount)}/mo` : null;
                  })
                    .filter(Boolean)
                    .join(" · ")}
                </span>
              </div>
              <div className="account-actions">
                {CLAIMING_AGES.map((age) => {
                  const amount =
                    age === 62
                      ? estimate.estimate_at_62
                      : age === 67
                        ? estimate.estimate_at_67
                        : estimate.estimate_at_70;
                  if (amount == null) return null;
                  return (
                    <Button
                      key={age}
                      variant="ghost"
                      onClick={() => handleUseEstimate(estimate, age, amount)}
                    >
                      Use age {age}
                    </Button>
                  );
                })}
                <Button variant="ghost" onClick={() => handleDelete(estimate)}>
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
