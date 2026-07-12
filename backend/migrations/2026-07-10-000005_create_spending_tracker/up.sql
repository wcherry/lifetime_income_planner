-- Spending Tracker (transaction-level CSV import + categorization): distinct
-- from the planned-budget "Spending" page (spending_items table). Users
-- import a month's worth of bank/credit-card transactions, categorize them,
-- and browse the history over time.
CREATE TABLE spending_tracker_categories (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT,

    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    is_predefined BOOLEAN NOT NULL DEFAULT 0,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE spending_tracker_imports (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    year INTEGER NOT NULL,
    month INTEGER NOT NULL,
    source_filename TEXT,
    row_count INTEGER NOT NULL,
    duplicate_count INTEGER NOT NULL,
    skipped_count INTEGER NOT NULL,

    imported_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_spending_tracker_imports_user_year_month ON spending_tracker_imports (user_id, year, month);

CREATE TABLE spending_tracker_transactions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    import_id TEXT NOT NULL,

    year INTEGER NOT NULL,
    month INTEGER NOT NULL,
    transaction_date DATE NOT NULL,
    description TEXT NOT NULL,
    amount DOUBLE NOT NULL,
    category_id TEXT,
    dedupe_key TEXT NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (import_id) REFERENCES spending_tracker_imports (id) ON DELETE CASCADE,
    FOREIGN KEY (category_id) REFERENCES spending_tracker_categories (id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_spending_tracker_transactions_user_dedupe ON spending_tracker_transactions (user_id, dedupe_key);
CREATE INDEX idx_spending_tracker_transactions_user_year_month ON spending_tracker_transactions (user_id, year, month);
