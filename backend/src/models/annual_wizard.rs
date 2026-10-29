//! Annual planning wizard persistence (roadmap Phase 5, feature 8): the
//! wizard itself is a guided checklist that sequences through the existing
//! Profile/Accounts/Income/Spending/Life events/Assumptions pages rather
//! than re-implementing their forms, so this only persists whether a given
//! year's planning pass has been marked complete.

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::schema::annual_plan_reviews;

/// A completed annual-review marker row, as persisted.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = annual_plan_reviews)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AnnualPlanReview {
    pub id: String,
    pub user_id: String,
    pub year: i32,
    pub completed_at: NaiveDateTime,
}

/// Insertable row for marking a year's planning complete.
#[derive(Insertable)]
#[diesel(table_name = annual_plan_reviews)]
pub struct NewAnnualPlanReview {
    pub id: String,
    pub user_id: String,
    pub year: i32,
    pub completed_at: NaiveDateTime,
}

/// One step of the guided checklist, linking out to an existing page.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AnnualWizardStep {
    pub key: String,
    pub label: String,
    /// Frontend route this step links to.
    pub route: String,
    /// A live status hint pulled from the user's actual data (e.g. "Last
    /// updated 2026-01-04" or "3 accounts"), so the checklist isn't static.
    pub hint: String,
}

/// The full wizard state for the current year.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AnnualWizardResponse {
    pub year: i32,
    /// Whether this year's planning pass has already been marked complete.
    pub completed: bool,
    pub steps: Vec<AnnualWizardStep>,
}
