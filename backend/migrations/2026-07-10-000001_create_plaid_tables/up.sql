-- Financial account aggregation (roadmap Phase 6, features 1-2): a linked
-- Plaid "item" (one bank login) optionally tied to one of the user's own
-- accounts, plus the transactions pulled in on each sync. `plaid_access_token`
-- never leaves the backend.
CREATE TABLE plaid_items (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    account_id TEXT,

    plaid_item_id TEXT NOT NULL,
    plaid_access_token TEXT NOT NULL,
    institution_name TEXT NOT NULL,
    status TEXT NOT NULL,
    last_synced_at TIMESTAMP,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES accounts (id) ON DELETE SET NULL
);

CREATE INDEX idx_plaid_items_user ON plaid_items (user_id);

CREATE TABLE plaid_transactions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    plaid_item_id TEXT NOT NULL,
    account_id TEXT,

    plaid_transaction_id TEXT NOT NULL,
    posted_date DATE NOT NULL,
    amount DOUBLE NOT NULL,
    description TEXT NOT NULL,
    category TEXT,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (plaid_item_id) REFERENCES plaid_items (id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES accounts (id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_plaid_transactions_plaid_id ON plaid_transactions (plaid_transaction_id);
CREATE INDEX idx_plaid_transactions_item ON plaid_transactions (plaid_item_id);
