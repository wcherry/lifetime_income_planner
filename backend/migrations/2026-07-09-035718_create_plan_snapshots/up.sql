-- Historical scenario snapshots (roadmap Phase 4, feature 7). Each row is a
-- past version of a plan's data. `plans.snapshot` always holds the *current*
-- version; updating a plan's data (rather than just renaming it) archives
-- the displaced snapshot here first, so a scenario accumulates a timeline as
-- it evolves instead of silently overwriting its history.
CREATE TABLE plan_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL,
    user_id TEXT NOT NULL,

    snapshot TEXT NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (plan_id) REFERENCES plans (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_plan_snapshots_plan_id ON plan_snapshots (plan_id);
