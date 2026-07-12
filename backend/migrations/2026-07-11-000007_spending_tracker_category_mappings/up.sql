-- "Learned" category mappings (CSV category label -> one of the user's
-- categories): once a user categorizes a transaction that came in with a
-- CSV category label (single or bulk), that choice is remembered here so
-- the next import of the same label applies it automatically instead of
-- asking for review again.
ALTER TABLE spending_tracker_transactions
    ADD COLUMN source_category_label TEXT;

CREATE TABLE spending_tracker_category_mappings (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    label TEXT NOT NULL,
    normalized_label TEXT NOT NULL,
    category_id TEXT NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (category_id) REFERENCES spending_tracker_categories (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_spending_tracker_category_mappings_user_label
    ON spending_tracker_category_mappings (user_id, normalized_label);
