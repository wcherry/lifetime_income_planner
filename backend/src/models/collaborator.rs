//! Collaboration (roadmap Phase 6, feature 7): grants another registered
//! user read-write ("spouse") or read-only ("advisor") access to the
//! owner's data.

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::collaborators;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CollaboratorRole {
    /// Full read-write access, as if editing their own plan.
    Spouse,
    /// Read-only access; write requests are rejected by the auth extractor.
    Advisor,
}

impl CollaboratorRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            CollaboratorRole::Spouse => "spouse",
            CollaboratorRole::Advisor => "advisor",
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = collaborators)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Collaborator {
    pub id: String,
    pub owner_user_id: String,
    pub collaborator_user_id: String,
    pub invited_email: String,
    pub role: String,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = collaborators)]
pub struct NewCollaborator {
    pub id: String,
    pub owner_user_id: String,
    pub collaborator_user_id: String,
    pub invited_email: String,
    pub role: String,
    pub status: String,
    pub updated_at: NaiveDateTime,
}

/// Invite another registered user to collaborate. There's no email delivery
/// infra, so the invitee must already have an account.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct InviteCollaboratorRequest {
    #[validate(email(message = "must be a valid email address"))]
    #[schema(example = "spouse@example.com")]
    pub email: String,
    pub role: CollaboratorRole,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CollaboratorResponse {
    pub id: String,
    pub email: String,
    pub role: String,
    pub status: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
}

impl From<Collaborator> for CollaboratorResponse {
    fn from(c: Collaborator) -> Self {
        CollaboratorResponse {
            id: c.id,
            email: c.invited_email,
            role: c.role,
            status: c.status,
            created_at: c.created_at,
        }
    }
}

/// A pending invitation addressed to the caller, awaiting accept/decline.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InvitationResponse {
    pub id: String,
    pub owner_email: String,
    pub role: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
}

/// A context the caller can act as: their own data, or another owner's data
/// they've been granted access to. Drives the frontend's context switcher.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CollaborationContext {
    /// The user id to send as `X-Context-User` (the caller's own id for
    /// "self").
    pub user_id: String,
    pub label: String,
    /// `"owner"`, `"spouse"`, or `"advisor"`.
    pub role: String,
}
