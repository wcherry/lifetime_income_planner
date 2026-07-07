use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::life_events;

/// Kind of life event, mirroring the roadmap's Life Events Engine examples.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifeEventType {
    SellHouse,
    BuyHome,
    Inheritance,
    Downsize,
    StartMedicare,
    ClaimSocialSecurity,
    PayOffMortgage,
    Relocate,
    LargePurchase,
    Gift,
    DeathOfSpouse,
    Other,
}

impl LifeEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifeEventType::SellHouse => "sell_house",
            LifeEventType::BuyHome => "buy_home",
            LifeEventType::Inheritance => "inheritance",
            LifeEventType::Downsize => "downsize",
            LifeEventType::StartMedicare => "start_medicare",
            LifeEventType::ClaimSocialSecurity => "claim_social_security",
            LifeEventType::PayOffMortgage => "pay_off_mortgage",
            LifeEventType::Relocate => "relocate",
            LifeEventType::LargePurchase => "large_purchase",
            LifeEventType::Gift => "gift",
            LifeEventType::DeathOfSpouse => "death_of_spouse",
            LifeEventType::Other => "other",
        }
    }
}

/// Whether the event's cash flow comes in or goes out.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CashFlowDirection {
    Inflow,
    Outflow,
}

impl CashFlowDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            CashFlowDirection::Inflow => "inflow",
            CashFlowDirection::Outflow => "outflow",
        }
    }
}

/// How often a life event repeats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventRecurrence {
    OneTime,
    Monthly,
    Annual,
}

impl EventRecurrence {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventRecurrence::OneTime => "one_time",
            EventRecurrence::Monthly => "monthly",
            EventRecurrence::Annual => "annual",
        }
    }
}

/// Persisted life event row.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = life_events)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct LifeEvent {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub event_type: String,
    pub event_date: NaiveDate,
    pub direction: String,
    pub amount: f64,
    pub taxable: bool,
    pub inflation_adjusted: bool,
    pub recurrence: String,
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = life_events)]
pub struct NewLifeEvent {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub event_type: String,
    pub event_date: NaiveDate,
    pub direction: String,
    pub amount: f64,
    pub taxable: bool,
    pub inflation_adjusted: bool,
    pub recurrence: String,
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub updated_at: NaiveDateTime,
}

/// Request body for creating or updating a life event.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct LifeEventRequest {
    #[validate(length(min = 1, max = 120, message = "is required"))]
    #[schema(example = "Sell the lake house")]
    pub name: String,

    pub event_type: LifeEventType,

    #[schema(value_type = String, format = Date, example = "2032-06-01")]
    pub event_date: NaiveDate,

    pub direction: CashFlowDirection,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 350000.0)]
    pub amount: f64,

    #[serde(default)]
    pub taxable: bool,

    #[serde(default)]
    pub inflation_adjusted: bool,

    #[serde(default = "default_recurrence")]
    pub recurrence: EventRecurrence,

    #[schema(value_type = Option<String>, format = Date)]
    pub end_date: Option<NaiveDate>,

    #[validate(length(max = 500))]
    pub notes: Option<String>,
}

fn default_recurrence() -> EventRecurrence {
    EventRecurrence::OneTime
}

/// API view of a life event, including the signed cash flow.
#[derive(Serialize, ToSchema)]
pub struct LifeEventResponse {
    pub id: String,
    pub name: String,
    pub event_type: String,
    #[schema(value_type = String, format = Date)]
    pub event_date: NaiveDate,
    pub direction: String,
    pub amount: f64,
    /// `amount` signed by direction: positive for inflows, negative for outflows.
    pub signed_amount: f64,
    pub taxable: bool,
    pub inflation_adjusted: bool,
    pub recurrence: String,
    #[schema(value_type = Option<String>, format = Date)]
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: NaiveDateTime,
}

impl From<LifeEvent> for LifeEventResponse {
    fn from(e: LifeEvent) -> Self {
        let signed_amount = if e.direction == CashFlowDirection::Outflow.as_str() {
            -e.amount
        } else {
            e.amount
        };
        LifeEventResponse {
            id: e.id,
            name: e.name,
            event_type: e.event_type,
            event_date: e.event_date,
            direction: e.direction,
            amount: e.amount,
            signed_amount,
            taxable: e.taxable,
            inflation_adjusted: e.inflation_adjusted,
            recurrence: e.recurrence,
            end_date: e.end_date,
            notes: e.notes,
            updated_at: e.updated_at,
        }
    }
}
