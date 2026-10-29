//! Task completion persistence (roadmap Phase 5, feature 5): task *content*
//! is always derived fresh by `crate::tasks::derive_tasks` from projection
//! and insight data that already exists; this only persists which of those
//! generated tasks a user has checked off, keyed by the task's stable `key`.

use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::schema::task_completions;

/// A completed task row, as persisted.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = task_completions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct TaskCompletion {
    pub id: String,
    pub user_id: String,
    pub task_key: String,
    pub completed_at: NaiveDateTime,
}

/// Insertable row for marking a task complete.
#[derive(Insertable)]
#[diesel(table_name = task_completions)]
pub struct NewTaskCompletion {
    pub id: String,
    pub user_id: String,
    pub task_key: String,
    pub completed_at: NaiveDateTime,
}
