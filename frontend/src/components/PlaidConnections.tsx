import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { PlaidItem } from "../api/types";
import { formatCurrency } from "../data/format";
import { Alert, Button } from "./ui";

/**
 * Bank connections (roadmap Phase 6, feature 1): link a Plaid sandbox
 * institution and pull balances/transactions on demand. There's no Plaid
 * Link JS widget wired in — "Connect" exercises the exchange flow via
 * Plaid's sandbox test-token endpoint, which needs `PLAID_CLIENT_ID`/
 * `PLAID_SECRET` configured on the server.
 */
export function PlaidConnections() {
  const [items, setItems] = useState<PlaidItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [syncingId, setSyncingId] = useState<string | null>(null);

  async function refresh() {
    try {
      setItems(await api.listPlaidItems());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load bank connections");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleConnect() {
    setError(null);
    setStatus(null);
    setConnecting(true);
    try {
      await api.connectPlaidSandbox({
        institution_id: "ins_109508",
        institution_name: "First Platypus Bank (sandbox)",
      });
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to connect");
    } finally {
      setConnecting(false);
    }
  }

  async function handleSync(item: PlaidItem) {
    setError(null);
    setStatus(null);
    setSyncingId(item.id);
    try {
      const result = await api.syncPlaidItem(item.id);
      setStatus(
        `${item.institution_name}: ${result.new_transaction_count} new transaction${
          result.new_transaction_count === 1 ? "" : "s"
        }${result.updated_balance != null ? `, balance now ${formatCurrency(result.updated_balance)}` : ""}.`,
      );
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to sync");
    } finally {
      setSyncingId(null);
    }
  }

  async function handleUnlink(item: PlaidItem) {
    if (!window.confirm(`Unlink ${item.institution_name}?`)) return;
    setError(null);
    try {
      await api.deletePlaidItem(item.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to unlink");
    }
  }

  if (loading) return <p className="muted">Loading bank connections…</p>;

  return (
    <div className="stack">
      {error && <Alert kind="error">{error}</Alert>}
      {status && <Alert kind="success">{status}</Alert>}

      {items.length === 0 && (
        <p className="muted">
          No bank connections yet. Sandbox institutions require <code>PLAID_CLIENT_ID</code> and{" "}
          <code>PLAID_SECRET</code> to be configured on the server.
        </p>
      )}

      {items.length > 0 && (
        <div className="account-list">
          {items.map((item) => (
            <div className="account-row" key={item.id}>
              <div className="account-main">
                <span className="account-name">{item.institution_name}</span>
                <span className="account-meta muted">
                  {item.status}
                  {item.last_synced_at
                    ? ` · last synced ${new Date(item.last_synced_at).toLocaleString()}`
                    : " · never synced"}
                </span>
              </div>
              <div className="account-actions">
                <Button
                  variant="ghost"
                  disabled={syncingId === item.id}
                  onClick={() => handleSync(item)}
                >
                  {syncingId === item.id ? "Syncing…" : "Sync now"}
                </Button>
                <Button variant="ghost" onClick={() => handleUnlink(item)}>
                  Unlink
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="form-actions">
        <Button onClick={handleConnect} disabled={connecting}>
          {connecting ? "Connecting…" : "Connect sandbox institution"}
        </Button>
      </div>
    </div>
  );
}
