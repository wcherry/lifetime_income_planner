-- Annual planning wizard (roadmap Phase 5, feature 8): a single yearly
-- completion marker. The wizard itself is a guided checklist that sequences
-- through the existing Profile/Accounts/Income/Spending/Life events/
-- Assumptions pages rather than re-implementing their forms, so there's no
-- reconciliation math to store here — just whether this year's planning
-- pass has been marked done.
CREATE TABLE annual_plan_reviews (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    year INTEGER NOT NULL,

    completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_annual_plan_reviews_user_year ON annual_plan_reviews (user_id, year);
