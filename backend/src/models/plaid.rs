//! Financial account aggregation (roadmap Phase 6, features 1-2): a linked
//! Plaid "item" (one bank login), optionally tied to one of the user's own
//! accounts, plus the transactions pulled in on each sync.

use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema::{plaid_items, plaid_transactions};

/// Persisted linked-institution row. `plaid_access_token` never leaves the
/// backend — only [`PlaidItemResponse`] is ever serialized to a client.
#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = plaid_items)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct PlaidItem {
    pub id: String,
    pub user_id: String,
    pub account_id: Option<String>,
    pub plaid_item_id: String,
    pub plaid_access_token: String,
    pub institution_name: String,
    pub status: String,
    pub last_synced_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = plaid_items)]
pub struct NewPlaidItem {
    pub id: String,
    pub user_id: String,
    pub account_id: Option<String>,
    pub plaid_item_id: String,
    pub plaid_access_token: String,
    pub institution_name: String,
    pub status: String,
    pub updated_at: NaiveDateTime,
}

/// Connect a sandbox institution (roadmap Phase 6, feature 1). There is no
/// Plaid Link JS widget wired into the frontend — Plaid's
/// `/sandbox/public_token/create` endpoint generates a test `public_token`
/// server-side, so the whole exchange flow is exercisable without a hosted
/// UI. `link_account_id`, if given, ties the item to an existing account
/// instead of creating a new one.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlaidSandboxConnectRequest {
    #[schema(example = "ins_109508")]
    pub institution_id: String,
    #[schema(example = "First Platypus Bank")]
    pub institution_name: String,
    pub link_account_id: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PlaidItemResponse {
    pub id: String,
    pub account_id: Option<String>,
    pub institution_name: String,
    pub status: String,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_synced_at: Option<NaiveDateTime>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: NaiveDateTime,
}

impl From<PlaidItem> for PlaidItemResponse {
    fn from(p: PlaidItem) -> Self {
        PlaidItemResponse {
            id: p.id,
            account_id: p.account_id,
            institution_name: p.institution_name,
            status: p.status,
            last_synced_at: p.last_synced_at,
            created_at: p.created_at,
        }
    }
}

/// Result of a sync: the refreshed item plus how many new transactions were
/// pulled in.
#[derive(Serialize, ToSchema)]
pub struct PlaidSyncResponse {
    pub item: PlaidItemResponse,
    pub new_transaction_count: usize,
    pub updated_balance: Option<f64>,
}

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = plaid_transactions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct PlaidTransaction {
    pub id: String,
    pub user_id: String,
    pub plaid_item_id: String,
    pub account_id: Option<String>,
    pub plaid_transaction_id: String,
    pub posted_date: NaiveDate,
    pub amount: f64,
    pub description: String,
    pub category: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = plaid_transactions)]
pub struct NewPlaidTransaction {
    pub id: String,
    pub user_id: String,
    pub plaid_item_id: String,
    pub account_id: Option<String>,
    pub plaid_transaction_id: String,
    pub posted_date: NaiveDate,
    pub amount: f64,
    pub description: String,
    pub category: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PlaidTransactionResponse {
    pub id: String,
    #[schema(value_type = String, format = Date)]
    pub posted_date: NaiveDate,
    pub amount: f64,
    pub description: String,
    pub category: Option<String>,
}

impl From<PlaidTransaction> for PlaidTransactionResponse {
    fn from(t: PlaidTransaction) -> Self {
        PlaidTransactionResponse {
            id: t.id,
            posted_date: t.posted_date,
            amount: t.amount,
            description: t.description,
            category: t.category,
        }
    }
}
