-- Task lists and reminders (roadmap Phase 5, feature 5): tasks themselves
-- (e.g. "Take a withdrawal", "Pay estimated taxes") are always derived fresh
-- from the projection/insights data that already exists — this table only
-- persists which of those generated tasks a user has checked off, keyed by
-- a stable, deterministic `task_key` (e.g. "estimated_tax:2026-Q1").
CREATE TABLE task_completions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,

    task_key TEXT NOT NULL,

    completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_task_completions_user_key ON task_completions (user_id, task_key);
