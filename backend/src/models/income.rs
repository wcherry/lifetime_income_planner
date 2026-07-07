use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::models::spending::annualize;
use crate::schema::income_sources;

/// Type of income source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncomeType {
    SocialSecurity,
    Pension,
    Rental,
    Royalties,
    Annuity,
    Employment,
    Consulting,
    PartTime,
}

impl IncomeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            IncomeType::SocialSecurity => "social_security",
            IncomeType::Pension => "pension",
            IncomeType::Rental => "rental",
            IncomeType::Royalties => "royalties",
            IncomeType::Annuity => "annuity",
            IncomeType::Employment => "employment",
            IncomeType::Consulting => "consulting",
            IncomeType::PartTime => "part_time",
        }
    }
}

/// How often the income is received.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncomeFrequency {
    Monthly,
    Annual,
}

impl IncomeFrequency {
    pub fn as_str(&self) -> &'static str {
        match self {
            IncomeFrequency::Monthly => "monthly",
            IncomeFrequency::Annual => "annual",
        }
    }
}

/// How the income is taxed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Taxability {
    Taxable,
    PartiallyTaxable,
    TaxFree,
}

impl Taxability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Taxability::Taxable => "taxable",
            Taxability::PartiallyTaxable => "partially_taxable",
            Taxability::TaxFree => "tax_free",
        }
    }
}

/// Who receives the income.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncomeOwner {
    #[serde(rename = "self")]
    Owner,
    Spouse,
    Joint,
}

impl IncomeOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            IncomeOwner::Owner => "self",
            IncomeOwner::Spouse => "spouse",
            IncomeOwner::Joint => "joint",
        }
    }
}

/// Persisted income row.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = income_sources)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct IncomeSource {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub income_type: String,
    pub owner: String,
    pub amount: f64,
    pub frequency: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub growth_rate: f64,
    pub cola: bool,
    pub taxability: String,
    pub notes: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = income_sources)]
pub struct NewIncomeSource {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub income_type: String,
    pub owner: String,
    pub amount: f64,
    pub frequency: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub growth_rate: f64,
    pub cola: bool,
    pub taxability: String,
    pub notes: Option<String>,
    pub updated_at: NaiveDateTime,
}

/// Request body for creating or updating an income source.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct IncomeRequest {
    #[validate(length(min = 1, max = 120, message = "is required"))]
    #[schema(example = "Social Security")]
    pub name: String,

    pub income_type: IncomeType,
    pub owner: IncomeOwner,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 2800.0)]
    pub amount: f64,

    pub frequency: IncomeFrequency,

    #[schema(value_type = String, format = Date, example = "2032-01-01")]
    pub start_date: NaiveDate,

    #[schema(value_type = Option<String>, format = Date)]
    pub end_date: Option<NaiveDate>,

    #[serde(default)]
    #[validate(range(min = -100.0, max = 100.0, message = "must be between -100 and 100"))]
    pub growth_rate: f64,

    #[serde(default)]
    pub cola: bool,

    pub taxability: Taxability,

    #[validate(length(max = 500))]
    pub notes: Option<String>,
}

/// API view of an income source, including the annualized amount.
#[derive(Serialize, ToSchema)]
pub struct IncomeResponse {
    pub id: String,
    pub name: String,
    pub income_type: String,
    pub owner: String,
    pub amount: f64,
    pub frequency: String,
    pub annual_amount: f64,
    #[schema(value_type = String, format = Date)]
    pub start_date: NaiveDate,
    #[schema(value_type = Option<String>, format = Date)]
    pub end_date: Option<NaiveDate>,
    pub growth_rate: f64,
    pub cola: bool,
    pub taxability: String,
    pub notes: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: NaiveDateTime,
}

impl From<IncomeSource> for IncomeResponse {
    fn from(i: IncomeSource) -> Self {
        let annual_amount = annualize(i.amount, &i.frequency);
        IncomeResponse {
            id: i.id,
            name: i.name,
            income_type: i.income_type,
            owner: i.owner,
            amount: i.amount,
            annual_amount,
            frequency: i.frequency,
            start_date: i.start_date,
            end_date: i.end_date,
            growth_rate: i.growth_rate,
            cola: i.cola,
            taxability: i.taxability,
            notes: i.notes,
            updated_at: i.updated_at,
        }
    }
}
