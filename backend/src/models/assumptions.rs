use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::assumptions;

/// Default planning assumptions, used when a user has not saved their own yet.
pub const DEFAULT_INFLATION_RATE: f64 = 2.5;
pub const DEFAULT_INVESTMENT_RETURN_RATE: f64 = 6.0;
pub const DEFAULT_HEALTHCARE_INFLATION_RATE: f64 = 4.5;
pub const DEFAULT_SOCIAL_SECURITY_COLA_RATE: f64 = 2.0;

/// Persisted assumptions row (one per user).
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = assumptions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Assumptions {
    pub id: String,
    pub user_id: String,
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Insertable row used when assumptions are first created.
#[derive(Insertable)]
#[diesel(table_name = assumptions)]
pub struct NewAssumptions {
    pub id: String,
    pub user_id: String,
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    pub updated_at: NaiveDateTime,
}

/// Request body for creating or replacing the planning assumptions.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AssumptionsRequest {
    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 2.5)]
    pub inflation_rate: f64,

    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 6.0)]
    pub investment_return_rate: f64,

    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 4.5)]
    pub healthcare_inflation_rate: f64,

    #[validate(range(min = -20.0, max = 30.0, message = "must be between -20 and 30"))]
    #[schema(example = 2.0)]
    pub social_security_cola_rate: f64,
}

/// API view of the planning assumptions.
#[derive(Serialize, ToSchema)]
pub struct AssumptionsResponse {
    pub inflation_rate: f64,
    pub investment_return_rate: f64,
    pub healthcare_inflation_rate: f64,
    pub social_security_cola_rate: f64,
    /// True when no assumptions have been saved yet and defaults are being returned.
    pub is_default: bool,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub updated_at: Option<NaiveDateTime>,
}

impl AssumptionsResponse {
    /// The response returned before a user has saved any assumptions of their own.
    pub fn defaults() -> Self {
        AssumptionsResponse {
            inflation_rate: DEFAULT_INFLATION_RATE,
            investment_return_rate: DEFAULT_INVESTMENT_RETURN_RATE,
            healthcare_inflation_rate: DEFAULT_HEALTHCARE_INFLATION_RATE,
            social_security_cola_rate: DEFAULT_SOCIAL_SECURITY_COLA_RATE,
            is_default: true,
            updated_at: None,
        }
    }
}

impl From<Assumptions> for AssumptionsResponse {
    fn from(a: Assumptions) -> Self {
        AssumptionsResponse {
            inflation_rate: a.inflation_rate,
            investment_return_rate: a.investment_return_rate,
            healthcare_inflation_rate: a.healthcare_inflation_rate,
            social_security_cola_rate: a.social_security_cola_rate,
            is_default: false,
            updated_at: Some(a.updated_at),
        }
    }
}
