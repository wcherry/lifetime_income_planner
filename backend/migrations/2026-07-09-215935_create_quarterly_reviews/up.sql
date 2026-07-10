-- Quarterly reviews (roadmap Phase 5, features 1-4): a completed comparison
-- of a quarter's actual income/spending/tax and each account's actual ending
-- balance against the projection's previously-planned figures for that same
-- quarter. Completing a review immediately overwrites live account balances
-- with the entered actuals (the "automatic recalculation") — this table is
-- the audit trail of what actually happened, not a draft/staging area.
CREATE TABLE quarterly_reviews (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    year INTEGER NOT NULL,
    quarter INTEGER NOT NULL,

    planned_income DOUBLE NOT NULL,
    planned_spending DOUBLE NOT NULL,
    planned_tax DOUBLE NOT NULL,
    planned_withdrawal DOUBLE NOT NULL,

    actual_income DOUBLE NOT NULL,
    actual_spending DOUBLE NOT NULL,
    actual_tax DOUBLE NOT NULL,
    actual_balances TEXT NOT NULL,

    notes TEXT,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_quarterly_reviews_user_period ON quarterly_reviews (user_id, year, quarter);
