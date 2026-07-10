import { useEffect, useState, type FormEvent } from "react";
import { api } from "../api/client";
import type { Collaborator, CollaboratorRole, Invitation } from "../api/types";
import { Alert, Button, Card, Field, Select, TextInput } from "../components/ui";

function roleLabel(role: CollaboratorRole | string): string {
  return role === "advisor" ? "Advisor (read-only)" : "Spouse (full access)";
}

export function CollaborationPage() {
  const [collaborators, setCollaborators] = useState<Collaborator[]>([]);
  const [invitations, setInvitations] = useState<Invitation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [inviting, setInviting] = useState(false);

  const [email, setEmail] = useState("");
  const [role, setRole] = useState<CollaboratorRole>("spouse");

  async function refresh() {
    try {
      const [collabs, invites] = await Promise.all([
        api.listCollaborators(),
        api.listInvitations(),
      ]);
      setCollaborators(collabs);
      setInvitations(invites);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load collaborators");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleInvite(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setStatus(null);
    setInviting(true);
    try {
      await api.inviteCollaborator({ email: email.trim(), role });
      setEmail("");
      setStatus(`Invited ${email.trim()}.`);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to invite");
    } finally {
      setInviting(false);
    }
  }

  async function handleRevoke(c: Collaborator) {
    if (!window.confirm(`Revoke access for ${c.email}?`)) return;
    setError(null);
    try {
      await api.revokeCollaborator(c.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to revoke");
    }
  }

  async function handleAccept(invite: Invitation) {
    setError(null);
    setStatus(null);
    try {
      await api.acceptInvitation(invite.id);
      setStatus(
        `Accepted — use the context switcher in the header to view ${invite.owner_email}'s plan.`,
      );
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to accept");
    }
  }

  async function handleDecline(invite: Invitation) {
    setError(null);
    try {
      await api.declineInvitation(invite.id);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to decline");
    }
  }

  if (loading) return <p className="muted">Loading collaborators…</p>;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Collaboration</h1>
          <p className="muted">
            Give a spouse full access or an advisor read-only access to your plan.
          </p>
        </div>
      </div>

      {error && <Alert kind="error">{error}</Alert>}
      {status && <Alert kind="success">{status}</Alert>}

      {invitations.length > 0 && (
        <Card title="Invitations for you">
          <div className="account-list">
            {invitations.map((invite) => (
              <div className="account-row" key={invite.id}>
                <div className="account-main">
                  <span className="account-name">{invite.owner_email}</span>
                  <span className="account-meta muted">{roleLabel(invite.role)}</span>
                </div>
                <div className="account-actions">
                  <Button variant="ghost" onClick={() => handleAccept(invite)}>
                    Accept
                  </Button>
                  <Button variant="ghost" onClick={() => handleDecline(invite)}>
                    Decline
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </Card>
      )}

      <Card title="Invite a collaborator">
        <form onSubmit={handleInvite} className="account-form">
          <div className="grid-2">
            <Field label="Email" htmlFor="invite_email">
              <TextInput
                id="invite_email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="spouse@example.com"
                required
              />
            </Field>
            <Field label="Role" htmlFor="invite_role" hint="They must already have an account.">
              <Select
                id="invite_role"
                value={role}
                onChange={(e) => setRole(e.target.value as CollaboratorRole)}
              >
                <option value="spouse">Spouse (full access)</option>
                <option value="advisor">Advisor (read-only)</option>
              </Select>
            </Field>
          </div>
          <div className="form-actions">
            <Button type="submit" disabled={inviting}>
              {inviting ? "Inviting…" : "Invite"}
            </Button>
          </div>
        </form>
      </Card>

      {collaborators.length === 0 ? (
        <Card>
          <p className="muted center">No one else has access to your plan yet.</p>
        </Card>
      ) : (
        <div className="account-list">
          {collaborators.map((c) => (
            <div className="account-row" key={c.id}>
              <div className="account-main">
                <span className="account-name">{c.email}</span>
                <span className="account-meta muted">
                  {roleLabel(c.role)} · {c.status}
                </span>
              </div>
              <div className="account-actions">
                <Button variant="ghost" onClick={() => handleRevoke(c)}>
                  Revoke
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
