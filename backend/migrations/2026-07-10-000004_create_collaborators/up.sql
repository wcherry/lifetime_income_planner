-- Collaboration (roadmap Phase 6, feature 7): grants another registered
-- user read-write ("spouse") or read-only ("advisor") access to the owner's
-- data. There's no email delivery infra, so the invitee must already have an
-- account; the grant starts "pending" until they accept it.
CREATE TABLE collaborators (
    id TEXT PRIMARY KEY NOT NULL,
    owner_user_id TEXT NOT NULL,
    collaborator_user_id TEXT NOT NULL,
    invited_email TEXT NOT NULL,
    role TEXT NOT NULL,
    status TEXT NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (collaborator_user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_collaborators_owner_collaborator ON collaborators (owner_user_id, collaborator_user_id);
CREATE INDEX idx_collaborators_collaborator ON collaborators (collaborator_user_id);
