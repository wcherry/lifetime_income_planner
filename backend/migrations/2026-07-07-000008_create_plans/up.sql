CREATE TABLE plans (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    -- User-chosen name for the saved plan (e.g. "Baseline", "Retire at 62").
    name TEXT NOT NULL,

    -- JSON snapshot of the working set at save time: profile, assumptions,
    -- accounts, income sources, spending items, and life events. Stored as a
    -- single opaque document so a saved plan is independent of later edits to
    -- the live working set.
    snapshot TEXT NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_plans_user_id ON plans (user_id);
