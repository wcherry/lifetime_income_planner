//! Thin async wrapper around Plaid's REST API (roadmap Phase 6, feature 1).
//! Every call needs `PLAID_CLIENT_ID`/`PLAID_SECRET`; callers are expected to
//! check `Config::plaid_client_id`/`plaid_secret` and return a clear
//! "not configured" error before reaching here (see
//! `handlers::plaid::require_plaid_config`).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub struct PlaidCredentials<'a> {
    pub client_id: &'a str,
    pub secret: &'a str,
    pub env: &'a str,
}

impl PlaidCredentials<'_> {
    fn base_url(&self) -> String {
        format!("https://{}.plaid.com", self.env)
    }
}

#[derive(Serialize)]
struct SandboxPublicTokenRequest<'a> {
    client_id: &'a str,
    secret: &'a str,
    institution_id: &'a str,
    initial_products: Vec<&'a str>,
}

#[derive(Deserialize)]
struct SandboxPublicTokenResponse {
    public_token: String,
}

#[derive(Serialize)]
struct ExchangeTokenRequest<'a> {
    client_id: &'a str,
    secret: &'a str,
    public_token: &'a str,
}

#[derive(Deserialize)]
struct ExchangeTokenResponse {
    access_token: String,
    item_id: String,
}

pub struct ExchangedItem {
    pub access_token: String,
    pub plaid_item_id: String,
}

#[derive(Serialize)]
struct BalanceGetRequest<'a> {
    client_id: &'a str,
    secret: &'a str,
    access_token: &'a str,
}

#[derive(Deserialize)]
struct BalanceGetResponse {
    accounts: Vec<PlaidAccount>,
}

#[derive(Deserialize)]
struct PlaidAccount {
    balances: PlaidBalances,
}

#[derive(Deserialize)]
struct PlaidBalances {
    current: Option<f64>,
}

#[derive(Serialize)]
struct TransactionsSyncRequest<'a> {
    client_id: &'a str,
    secret: &'a str,
    access_token: &'a str,
}

#[derive(Deserialize)]
struct TransactionsSyncResponse {
    added: Vec<PlaidTransactionRaw>,
}

/// A single transaction as returned by Plaid's `/transactions/sync`. Pub so
/// handlers can map it into a `NewPlaidTransaction` without re-parsing JSON.
#[derive(Deserialize, Debug, Clone)]
pub struct PlaidTransactionRaw {
    pub transaction_id: String,
    pub amount: f64,
    pub date: NaiveDate,
    pub name: String,
    pub category: Option<Vec<String>>,
}

async fn post_json<TReq: Serialize, TResp: for<'de> Deserialize<'de>>(
    creds: &PlaidCredentials<'_>,
    path: &str,
    body: &TReq,
) -> AppResult<TResp> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", creds.base_url(), path);
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Plaid request to {path} failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "Plaid returned {status} for {path}: {text}"
        )));
    }

    resp.json::<TResp>()
        .await
        .map_err(|e| AppError::Internal(format!("Plaid response from {path} was unparseable: {e}")))
}

/// Create a sandbox `public_token` for a test institution — stands in for
/// the hosted Plaid Link widget, which isn't wired into the frontend.
pub async fn create_sandbox_public_token(
    creds: &PlaidCredentials<'_>,
    institution_id: &str,
) -> AppResult<String> {
    let body = SandboxPublicTokenRequest {
        client_id: creds.client_id,
        secret: creds.secret,
        institution_id,
        initial_products: vec!["transactions"],
    };
    let resp: SandboxPublicTokenResponse =
        post_json(creds, "/sandbox/public_token/create", &body).await?;
    Ok(resp.public_token)
}

/// Exchange a `public_token` for a durable `access_token` + item id.
pub async fn exchange_public_token(
    creds: &PlaidCredentials<'_>,
    public_token: &str,
) -> AppResult<ExchangedItem> {
    let body = ExchangeTokenRequest {
        client_id: creds.client_id,
        secret: creds.secret,
        public_token,
    };
    let resp: ExchangeTokenResponse =
        post_json(creds, "/item/public_token/exchange", &body).await?;
    Ok(ExchangedItem {
        access_token: resp.access_token,
        plaid_item_id: resp.item_id,
    })
}

/// Fetch the current balance for an item's accounts. Sandbox/simple linked
/// items expose a single account, so this returns the first one found.
pub async fn get_current_balance(
    creds: &PlaidCredentials<'_>,
    access_token: &str,
) -> AppResult<Option<f64>> {
    let body = BalanceGetRequest {
        client_id: creds.client_id,
        secret: creds.secret,
        access_token,
    };
    let resp: BalanceGetResponse = post_json(creds, "/accounts/balance/get", &body).await?;
    Ok(resp.accounts.first().and_then(|a| a.balances.current))
}

/// Fetch newly-added transactions for an item since it was last synced.
/// Cursor-based paging isn't persisted — each sync re-fetches from Plaid's
/// start of history, and callers de-duplicate on `plaid_transaction_id`.
pub async fn get_added_transactions(
    creds: &PlaidCredentials<'_>,
    access_token: &str,
) -> AppResult<Vec<PlaidTransactionRaw>> {
    let body = TransactionsSyncRequest {
        client_id: creds.client_id,
        secret: creds.secret,
        access_token,
    };
    let resp: TransactionsSyncResponse = post_json(creds, "/transactions/sync", &body).await?;
    Ok(resp.added)
}
