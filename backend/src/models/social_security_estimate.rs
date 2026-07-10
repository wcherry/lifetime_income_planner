//! Social Security statement import (roadmap Phase 6, feature 4): the
//! estimated monthly benefit at ages 62/67/70 as shown on a real SSA
//! statement, so an income source can be generated from a chosen claiming
//! age instead of guessed by hand.

use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::social_security_estimates;

/// Whose benefit this statement estimates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SsEstimateOwner {
    #[serde(rename = "self")]
    Owner,
    Spouse,
}

impl SsEstimateOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            SsEstimateOwner::Owner => "self",
            SsEstimateOwner::Spouse => "spouse",
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = social_security_estimates)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SocialSecurityEstimate {
    pub id: String,
    pub user_id: String,
    pub owner: String,
    pub statement_date: NaiveDate,
    pub estimate_at_62: Option<f64>,
    pub estimate_at_67: Option<f64>,
    pub estimate_at_70: Option<f64>,
    pub source: String,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = social_security_estimates)]
pub struct NewSocialSecurityEstimate {
    pub id: String,
    pub user_id: String,
    pub owner: String,
    pub statement_date: NaiveDate,
    pub estimate_at_62: Option<f64>,
    pub estimate_at_67: Option<f64>,
    pub estimate_at_70: Option<f64>,
    pub source: String,
}

/// Import request mirroring the three claiming-age figures on a real SSA
/// statement. At least one estimate must be provided.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct ImportSocialSecurityEstimateRequest {
    pub owner: SsEstimateOwner,

    #[schema(value_type = String, format = Date, example = "2026-01-15")]
    pub statement_date: NaiveDate,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 2100.0)]
    pub estimate_at_62: Option<f64>,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 3000.0)]
    pub estimate_at_67: Option<f64>,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 3720.0)]
    pub estimate_at_70: Option<f64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SocialSecurityEstimateResponse {
    pub id: String,
    pub owner: String,
    #[schema(value_type = String, format = Date)]
    pub statement_date: NaiveDate,
    pub estimate_at_62: Option<f64>,
    pub estimate_at_67: Option<f64>,
    pub estimate_at_70: Option<f64>,
    pub source: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
}

impl From<SocialSecurityEstimate> for SocialSecurityEstimateResponse {
    fn from(e: SocialSecurityEstimate) -> Self {
        SocialSecurityEstimateResponse {
            id: e.id,
            owner: e.owner,
            statement_date: e.statement_date,
            estimate_at_62: e.estimate_at_62,
            estimate_at_67: e.estimate_at_67,
            estimate_at_70: e.estimate_at_70,
            source: e.source,
            created_at: e.created_at,
        }
    }
}
