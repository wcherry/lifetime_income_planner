import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { Account, AccountRequest } from "../api/types";
import { AccountForm } from "../components/AccountForm";
import { PlaidConnections } from "../components/PlaidConnections";
import { Alert, Button, Card } from "../components/ui";
import { accountTypeLabel, categoryLabel, formatCurrency, ownerLabel } from "../data/accounts";

type Editing = { mode: "new" } | { mode: "edit"; account: Account } | null;

export function AccountsPage() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing>(null);

  async function refresh() {
    try {
      setAccounts(await api.listAccounts());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load accounts");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSubmit(payload: AccountRequest) {
    if (editing?.mode === "edit") {
      await api.updateAccount(editing.account.id, payload);
    } else {
      await api.createAccount(payload);
    }
    setEditing(null);
    await refresh();
  }

  async function handleDelete(account: Account) {
    if (!window.confirm(`Delete "${account.name}"? This cannot be undone.`)) return;
    setError(null);
    try {
      await api.deleteAccount(account.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete account");
    }
  }

  if (loading) return <p className="muted">Loading accounts…</p>;

  const total = accounts.reduce((sum, a) => sum + a.current_balance, 0);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Accounts</h1>
          <p className="muted">
            {accounts.length} account{accounts.length === 1 ? "" : "s"} ·{" "}
            <strong>{formatCurrency(total)}</strong> total
          </p>
        </div>
        {!editing && <Button onClick={() => setEditing({ mode: "new" })}>Add account</Button>}
      </div>

      {error && <Alert kind="error">{error}</Alert>}

      {editing && (
        <Card title={editing.mode === "edit" ? "Edit account" : "New account"}>
          <AccountForm
            initial={editing.mode === "edit" ? editing.account : undefined}
            onSubmit={handleSubmit}
            onCancel={() => setEditing(null)}
          />
        </Card>
      )}

      {accounts.length === 0 && !editing && (
        <Card>
          <p className="muted center">
            No accounts yet. Add your brokerage, IRA, 401(k), Roth, HSA, and other accounts to build
            your plan.
          </p>
        </Card>
      )}

      {accounts.length > 0 && (
        <div className="account-list">
          {accounts.map((a) => (
            <div className="account-row" key={a.id}>
              <div className="account-main">
                <span className="account-name">{a.name}</span>
                <span className="account-meta muted">
                  {accountTypeLabel(a.account_type)} · {categoryLabel(a.category)} ·{" "}
                  {ownerLabel(a.owner)}
                </span>
              </div>
              <div className="account-figures">
                <span className="account-balance">{formatCurrency(a.current_balance)}</span>
                <span className="account-meta muted">{a.expected_roi}% ROI</span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => setEditing({ mode: "edit", account: a })}>
                  Edit
                </Button>
                <Button variant="ghost" onClick={() => handleDelete(a)}>
                  Delete
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Card title="Bank connections" collapsible defaultOpen={false}>
        <PlaidConnections />
      </Card>
    </div>
  );
}
