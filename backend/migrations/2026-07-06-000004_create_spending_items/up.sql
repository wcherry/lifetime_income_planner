CREATE TABLE spending_items (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    name TEXT NOT NULL,

    -- essential | discretionary | healthcare | travel | one_time | charity |
    -- taxes | home_maintenance | vehicle_replacement | large_purchase
    category TEXT NOT NULL,

    -- Amount per `frequency` period.
    amount DOUBLE NOT NULL,
    -- monthly | annual | one_time
    frequency TEXT NOT NULL,

    -- Whether the amount grows with the general inflation assumption.
    inflation_adjusted BOOL NOT NULL DEFAULT 1,

    -- Optional calendar-year bounds for when the expense applies.
    start_year INTEGER,
    end_year INTEGER,

    notes TEXT,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_spending_items_user_id ON spending_items (user_id);
