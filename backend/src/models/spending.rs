use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::spending_items;

/// Spending category, mirroring the roadmap's Spending Plan buckets.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendingCategory {
    Essential,
    Discretionary,
    Healthcare,
    Travel,
    OneTime,
    Charity,
    Taxes,
    HomeMaintenance,
    VehicleReplacement,
    LargePurchase,
}

impl SpendingCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpendingCategory::Essential => "essential",
            SpendingCategory::Discretionary => "discretionary",
            SpendingCategory::Healthcare => "healthcare",
            SpendingCategory::Travel => "travel",
            SpendingCategory::OneTime => "one_time",
            SpendingCategory::Charity => "charity",
            SpendingCategory::Taxes => "taxes",
            SpendingCategory::HomeMaintenance => "home_maintenance",
            SpendingCategory::VehicleReplacement => "vehicle_replacement",
            SpendingCategory::LargePurchase => "large_purchase",
        }
    }
}

/// How often a spending amount recurs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendingFrequency {
    Monthly,
    Annual,
    OneTime,
}

impl SpendingFrequency {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpendingFrequency::Monthly => "monthly",
            SpendingFrequency::Annual => "annual",
            SpendingFrequency::OneTime => "one_time",
        }
    }
}

/// Annualized amount for a given frequency (a one-time amount counts once).
pub fn annualize(amount: f64, frequency: &str) -> f64 {
    match frequency {
        "monthly" => amount * 12.0,
        _ => amount,
    }
}

/// Persisted spending row.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = spending_items)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SpendingItem {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub category: String,
    pub amount: f64,
    pub frequency: String,
    pub inflation_adjusted: bool,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub notes: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = spending_items)]
pub struct NewSpendingItem {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub category: String,
    pub amount: f64,
    pub frequency: String,
    pub inflation_adjusted: bool,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub notes: Option<String>,
    pub updated_at: NaiveDateTime,
}

/// Request body for creating or updating a spending item.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct SpendingRequest {
    #[validate(length(min = 1, max = 120, message = "is required"))]
    #[schema(example = "Groceries")]
    pub name: String,

    pub category: SpendingCategory,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 800.0)]
    pub amount: f64,

    pub frequency: SpendingFrequency,

    #[serde(default = "default_true")]
    pub inflation_adjusted: bool,

    #[validate(range(min = 1900, max = 2200))]
    pub start_year: Option<i32>,
    #[validate(range(min = 1900, max = 2200))]
    pub end_year: Option<i32>,

    #[validate(length(max = 500))]
    pub notes: Option<String>,
}

fn default_true() -> bool {
    true
}

/// API view of a spending item, including the annualized amount.
#[derive(Serialize, ToSchema)]
pub struct SpendingResponse {
    pub id: String,
    pub name: String,
    pub category: String,
    pub amount: f64,
    pub frequency: String,
    pub annual_amount: f64,
    pub inflation_adjusted: bool,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub notes: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: NaiveDateTime,
}

impl From<SpendingItem> for SpendingResponse {
    fn from(s: SpendingItem) -> Self {
        let annual_amount = annualize(s.amount, &s.frequency);
        SpendingResponse {
            id: s.id,
            name: s.name,
            category: s.category,
            amount: s.amount,
            annual_amount,
            frequency: s.frequency,
            inflation_adjusted: s.inflation_adjusted,
            start_year: s.start_year,
            end_year: s.end_year,
            notes: s.notes,
            updated_at: s.updated_at,
        }
    }
}
