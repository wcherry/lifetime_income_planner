use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::profiles;

/// Marital status supported by the planner.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaritalStatus {
    Single,
    Married,
    Widowed,
}

impl MaritalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MaritalStatus::Single => "single",
            MaritalStatus::Married => "married",
            MaritalStatus::Widowed => "widowed",
        }
    }
}

/// Federal tax filing status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilingStatus {
    Single,
    MarriedFilingJointly,
    MarriedFilingSeparately,
    HeadOfHousehold,
    QualifyingWidow,
}

impl FilingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FilingStatus::Single => "single",
            FilingStatus::MarriedFilingJointly => "married_filing_jointly",
            FilingStatus::MarriedFilingSeparately => "married_filing_separately",
            FilingStatus::HeadOfHousehold => "head_of_household",
            FilingStatus::QualifyingWidow => "qualifying_widow",
        }
    }
}

/// Persisted retirement profile row (one per user).
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = profiles)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Profile {
    pub id: String,
    pub user_id: String,
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: NaiveDate,
    pub marital_status: String,
    pub filing_status: String,
    pub state: String,
    pub retirement_date: NaiveDate,
    pub life_expectancy: i32,
    pub spouse_first_name: Option<String>,
    pub spouse_last_name: Option<String>,
    pub spouse_date_of_birth: Option<NaiveDate>,
    pub spouse_life_expectancy: Option<i32>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Insertable row used when a profile is first created.
#[derive(Insertable, AsChangeset)]
#[diesel(table_name = profiles)]
pub struct ProfileChangeset {
    pub id: String,
    pub user_id: String,
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: NaiveDate,
    pub marital_status: String,
    pub filing_status: String,
    pub state: String,
    pub retirement_date: NaiveDate,
    pub life_expectancy: i32,
    pub spouse_first_name: Option<String>,
    pub spouse_last_name: Option<String>,
    pub spouse_date_of_birth: Option<NaiveDate>,
    pub spouse_life_expectancy: Option<i32>,
    pub updated_at: NaiveDateTime,
}

/// Request body for creating or replacing a retirement profile.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct UpsertProfileRequest {
    #[validate(length(min = 1, max = 100, message = "is required"))]
    pub first_name: String,

    #[validate(length(min = 1, max = 100, message = "is required"))]
    pub last_name: String,

    #[schema(value_type = String, format = Date, example = "1965-04-12")]
    pub date_of_birth: NaiveDate,

    pub marital_status: MaritalStatus,
    pub filing_status: FilingStatus,

    #[validate(length(equal = 2, message = "must be a 2-letter state code"))]
    #[schema(example = "TX")]
    pub state: String,

    #[schema(value_type = String, format = Date, example = "2030-01-01")]
    pub retirement_date: NaiveDate,

    #[validate(range(min = 50, max = 120, message = "must be between 50 and 120"))]
    #[schema(example = 95)]
    pub life_expectancy: i32,

    // Required when marital_status = married (enforced in the handler).
    #[validate(length(max = 100))]
    pub spouse_first_name: Option<String>,

    #[validate(length(max = 100))]
    pub spouse_last_name: Option<String>,

    #[schema(value_type = Option<String>, format = Date)]
    pub spouse_date_of_birth: Option<NaiveDate>,

    #[validate(range(min = 50, max = 120, message = "must be between 50 and 120"))]
    pub spouse_life_expectancy: Option<i32>,
}

/// API view of a retirement profile.
#[derive(Serialize, ToSchema)]
pub struct ProfileResponse {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    #[schema(value_type = String, format = Date)]
    pub date_of_birth: NaiveDate,
    pub marital_status: String,
    pub filing_status: String,
    pub state: String,
    #[schema(value_type = String, format = Date)]
    pub retirement_date: NaiveDate,
    pub life_expectancy: i32,
    pub spouse_first_name: Option<String>,
    pub spouse_last_name: Option<String>,
    #[schema(value_type = Option<String>, format = Date)]
    pub spouse_date_of_birth: Option<NaiveDate>,
    pub spouse_life_expectancy: Option<i32>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: NaiveDateTime,
}

impl From<Profile> for ProfileResponse {
    fn from(p: Profile) -> Self {
        ProfileResponse {
            id: p.id,
            first_name: p.first_name,
            last_name: p.last_name,
            date_of_birth: p.date_of_birth,
            marital_status: p.marital_status,
            filing_status: p.filing_status,
            state: p.state,
            retirement_date: p.retirement_date,
            life_expectancy: p.life_expectancy,
            spouse_first_name: p.spouse_first_name,
            spouse_last_name: p.spouse_last_name,
            spouse_date_of_birth: p.spouse_date_of_birth,
            spouse_life_expectancy: p.spouse_life_expectancy,
            updated_at: p.updated_at,
        }
    }
}
