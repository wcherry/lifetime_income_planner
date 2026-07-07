use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::schema::accounts;

/// Tax treatment of an account.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountCategory {
    Taxable,
    TaxDeferred,
    TaxFree,
    Other,
}

impl AccountCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountCategory::Taxable => "taxable",
            AccountCategory::TaxDeferred => "tax_deferred",
            AccountCategory::TaxFree => "tax_free",
            AccountCategory::Other => "other",
        }
    }
}

/// Specific type of account.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Brokerage,
    Savings,
    Checking,
    MoneyMarket,
    Cd,
    Ira,
    #[serde(rename = "401k")]
    Traditional401k,
    #[serde(rename = "403b")]
    Traditional403b,
    #[serde(rename = "457")]
    Plan457,
    SepIra,
    RothIra,
    #[serde(rename = "roth_401k")]
    Roth401k,
    Hsa,
    Pension,
    CashValueLifeInsurance,
}

impl AccountType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountType::Brokerage => "brokerage",
            AccountType::Savings => "savings",
            AccountType::Checking => "checking",
            AccountType::MoneyMarket => "money_market",
            AccountType::Cd => "cd",
            AccountType::Ira => "ira",
            AccountType::Traditional401k => "401k",
            AccountType::Traditional403b => "403b",
            AccountType::Plan457 => "457",
            AccountType::SepIra => "sep_ira",
            AccountType::RothIra => "roth_ira",
            AccountType::Roth401k => "roth_401k",
            AccountType::Hsa => "hsa",
            AccountType::Pension => "pension",
            AccountType::CashValueLifeInsurance => "cash_value_life_insurance",
        }
    }
}

/// Who owns the account.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountOwner {
    #[serde(rename = "self")]
    Owner,
    Spouse,
    Joint,
}

impl AccountOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountOwner::Owner => "self",
            AccountOwner::Spouse => "spouse",
            AccountOwner::Joint => "joint",
        }
    }
}

/// Persisted account row.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = accounts)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Account {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub category: String,
    pub account_type: String,
    pub owner: String,
    pub current_balance: f64,
    pub expected_roi: f64,
    pub dividend_yield: f64,
    pub cost_basis: Option<f64>,
    pub allocation_stock_pct: Option<i32>,
    pub allocation_bond_pct: Option<i32>,
    pub allocation_cash_pct: Option<i32>,
    pub withdrawal_restrictions: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Insertable row for a new account.
#[derive(Insertable)]
#[diesel(table_name = accounts)]
pub struct NewAccount {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub category: String,
    pub account_type: String,
    pub owner: String,
    pub current_balance: f64,
    pub expected_roi: f64,
    pub dividend_yield: f64,
    pub cost_basis: Option<f64>,
    pub allocation_stock_pct: Option<i32>,
    pub allocation_bond_pct: Option<i32>,
    pub allocation_cash_pct: Option<i32>,
    pub withdrawal_restrictions: Option<String>,
    pub updated_at: NaiveDateTime,
}

/// Request body for creating or updating an account. Used for both POST and PUT.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AccountRequest {
    #[validate(length(min = 1, max = 120, message = "is required"))]
    #[schema(example = "Fidelity Brokerage")]
    pub name: String,

    pub category: AccountCategory,
    pub account_type: AccountType,
    pub owner: AccountOwner,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    #[schema(example = 250000.0)]
    pub current_balance: f64,

    #[validate(range(min = -100.0, max = 100.0, message = "must be between -100 and 100"))]
    #[schema(example = 6.5)]
    pub expected_roi: f64,

    #[serde(default)]
    #[validate(range(min = 0.0, max = 100.0, message = "must be between 0 and 100"))]
    #[schema(example = 1.8)]
    pub dividend_yield: f64,

    #[validate(range(min = 0.0, message = "cannot be negative"))]
    pub cost_basis: Option<f64>,

    #[validate(range(min = 0, max = 100))]
    pub allocation_stock_pct: Option<i32>,
    #[validate(range(min = 0, max = 100))]
    pub allocation_bond_pct: Option<i32>,
    #[validate(range(min = 0, max = 100))]
    pub allocation_cash_pct: Option<i32>,

    #[validate(length(max = 500))]
    pub withdrawal_restrictions: Option<String>,
}

/// API view of an account.
#[derive(Serialize, ToSchema)]
pub struct AccountResponse {
    pub id: String,
    pub name: String,
    pub category: String,
    pub account_type: String,
    pub owner: String,
    pub current_balance: f64,
    pub expected_roi: f64,
    pub dividend_yield: f64,
    pub cost_basis: Option<f64>,
    pub allocation_stock_pct: Option<i32>,
    pub allocation_bond_pct: Option<i32>,
    pub allocation_cash_pct: Option<i32>,
    pub withdrawal_restrictions: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: NaiveDateTime,
}

impl From<Account> for AccountResponse {
    fn from(a: Account) -> Self {
        AccountResponse {
            id: a.id,
            name: a.name,
            category: a.category,
            account_type: a.account_type,
            owner: a.owner,
            current_balance: a.current_balance,
            expected_roi: a.expected_roi,
            dividend_yield: a.dividend_yield,
            cost_basis: a.cost_basis,
            allocation_stock_pct: a.allocation_stock_pct,
            allocation_bond_pct: a.allocation_bond_pct,
            allocation_cash_pct: a.allocation_cash_pct,
            withdrawal_restrictions: a.withdrawal_restrictions,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}
