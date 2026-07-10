//! Financial account aggregation (roadmap Phase 6, features 1-2): link a
//! Plaid sandbox institution, then pull balances and transactions on demand.

use actix_web::{delete, get, post, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    NewAccount, NewPlaidItem, NewPlaidTransaction, PlaidItem, PlaidItemResponse,
    PlaidSandboxConnectRequest, PlaidSyncResponse,
};
use crate::plaid_client::{self, PlaidCredentials};
use crate::schema::{accounts, plaid_items, plaid_transactions};

/// Build Plaid credentials from config, or a clear error if unset.
fn require_plaid_config(config: &Config) -> AppResult<PlaidCredentials<'_>> {
    let client_id = config
        .plaid_client_id
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("Plaid is not configured on this server".into()))?;
    let secret = config
        .plaid_secret
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("Plaid is not configured on this server".into()))?;
    Ok(PlaidCredentials {
        client_id,
        secret,
        env: &config.plaid_env,
    })
}

/// Connect a sandbox institution (roadmap Phase 6, feature 1): exchanges a
/// server-generated sandbox `public_token` for an access token, then either
/// links it to an existing account or creates a new one.
#[utoipa::path(
    post,
    path = "/api/plaid/sandbox-connect",
    tag = "plaid",
    request_body = PlaidSandboxConnectRequest,
    responses(
        (status = 201, description = "Institution linked", body = PlaidItemResponse),
        (status = 400, description = "Plaid not configured, or validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plaid/sandbox-connect")]
pub async fn sandbox_connect(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    auth: AuthUser,
    body: web::Json<PlaidSandboxConnectRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    let creds = require_plaid_config(&config)?;

    let public_token =
        plaid_client::create_sandbox_public_token(&creds, &payload.institution_id).await?;
    let exchanged = plaid_client::exchange_public_token(&creds, &public_token).await?;
    let balance = plaid_client::get_current_balance(&creds, &exchanged.access_token)
        .await
        .unwrap_or(None);

    let user_id = auth.user_id.clone();
    let link_account_id = payload.link_account_id.clone();
    let institution_name = payload.institution_name.clone();
    let pool = pool.clone();

    let item = web::block(move || -> AppResult<PlaidItem> {
        let mut conn = pool.get()?;
        conn.transaction::<PlaidItem, AppError, _>(|conn| {
            let account_id = match link_account_id {
                Some(id) => {
                    let owned: i64 = accounts::table
                        .filter(accounts::id.eq(&id))
                        .filter(accounts::user_id.eq(&user_id))
                        .count()
                        .get_result(conn)?;
                    if owned == 0 {
                        return Err(AppError::NotFound("Account not found".into()));
                    }
                    if let Some(bal) = balance {
                        diesel::update(accounts::table.filter(accounts::id.eq(&id)))
                            .set((
                                accounts::current_balance.eq(bal),
                                accounts::updated_at.eq(Utc::now().naive_utc()),
                            ))
                            .execute(conn)?;
                    }
                    Some(id)
                }
                None => {
                    let new_account = NewAccount {
                        id: Uuid::new_v4().to_string(),
                        user_id: user_id.clone(),
                        name: institution_name.clone(),
                        category: "taxable".to_string(),
                        account_type: "checking".to_string(),
                        owner: "self".to_string(),
                        current_balance: balance.unwrap_or(0.0),
                        expected_roi: 0.0,
                        dividend_yield: 0.0,
                        cost_basis: None,
                        allocation_stock_pct: None,
                        allocation_bond_pct: None,
                        allocation_cash_pct: None,
                        withdrawal_restrictions: None,
                        updated_at: Utc::now().naive_utc(),
                    };
                    diesel::insert_into(accounts::table)
                        .values(&new_account)
                        .execute(conn)?;
                    Some(new_account.id)
                }
            };

            let id = Uuid::new_v4().to_string();
            let new_item = NewPlaidItem {
                id: id.clone(),
                user_id: user_id.clone(),
                account_id,
                plaid_item_id: exchanged.plaid_item_id.clone(),
                plaid_access_token: exchanged.access_token.clone(),
                institution_name: institution_name.clone(),
                status: "active".to_string(),
                updated_at: Utc::now().naive_utc(),
            };
            diesel::insert_into(plaid_items::table)
                .values(&new_item)
                .execute(conn)?;

            let item = plaid_items::table
                .filter(plaid_items::id.eq(&id))
                .select(PlaidItem::as_select())
                .first(conn)?;
            Ok(item)
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(PlaidItemResponse::from(item)))
}

/// List the caller's linked institutions.
#[utoipa::path(
    get,
    path = "/api/plaid/items",
    tag = "plaid",
    responses(
        (status = 200, description = "Linked institutions", body = [PlaidItemResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/plaid/items")]
pub async fn list_items(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<PlaidItem>> {
        let mut conn = pool.get()?;
        let rows = plaid_items::table
            .filter(plaid_items::user_id.eq(&auth.user_id))
            .order(plaid_items::created_at.desc())
            .select(PlaidItem::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<PlaidItemResponse> = rows.into_iter().map(PlaidItemResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Pull the latest balance and any new transactions for a linked item
/// (roadmap Phase 6, feature 2 — "automatic" here means on-demand, since
/// there's no background job scheduler in this deployment).
#[utoipa::path(
    post,
    path = "/api/plaid/items/{id}/sync",
    tag = "plaid",
    params(("id" = String, Path, description = "Plaid item id")),
    responses(
        (status = 200, description = "Sync result", body = PlaidSyncResponse),
        (status = 400, description = "Plaid not configured"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plaid/items/{id}/sync")]
pub async fn sync_item(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let creds = require_plaid_config(&config)?;

    let user_id = auth.user_id.clone();
    let pool_clone = pool.clone();
    let item = web::block(move || -> AppResult<PlaidItem> {
        let mut conn = pool_clone.get()?;
        plaid_items::table
            .filter(plaid_items::id.eq(&id))
            .filter(plaid_items::user_id.eq(&user_id))
            .select(PlaidItem::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Plaid item not found".into()))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let balance = plaid_client::get_current_balance(&creds, &item.plaid_access_token).await?;
    let transactions = plaid_client::get_added_transactions(&creds, &item.plaid_access_token).await?;

    let user_id = auth.user_id.clone();
    let item_id = item.id.clone();
    let account_id = item.account_id.clone();
    let pool = pool.clone();
    let (updated_item, new_count) = web::block(move || -> AppResult<(PlaidItem, usize)> {
        let mut conn = pool.get()?;
        conn.transaction::<(PlaidItem, usize), AppError, _>(|conn| {
            if let (Some(bal), Some(acc_id)) = (balance, &account_id) {
                diesel::update(accounts::table.filter(accounts::id.eq(acc_id)))
                    .set((
                        accounts::current_balance.eq(bal),
                        accounts::updated_at.eq(Utc::now().naive_utc()),
                    ))
                    .execute(conn)?;
            }

            let mut new_count = 0usize;
            for raw in &transactions {
                let new_txn = NewPlaidTransaction {
                    id: Uuid::new_v4().to_string(),
                    user_id: user_id.clone(),
                    plaid_item_id: item_id.clone(),
                    account_id: account_id.clone(),
                    plaid_transaction_id: raw.transaction_id.clone(),
                    posted_date: raw.date,
                    amount: raw.amount,
                    description: raw.name.clone(),
                    category: raw.category.as_ref().and_then(|c| c.first()).cloned(),
                };
                let inserted = diesel::insert_into(plaid_transactions::table)
                    .values(&new_txn)
                    .on_conflict(plaid_transactions::plaid_transaction_id)
                    .do_nothing()
                    .execute(conn)?;
                new_count += inserted;
            }

            diesel::update(plaid_items::table.filter(plaid_items::id.eq(&item_id)))
                .set((
                    plaid_items::last_synced_at.eq(Some(Utc::now().naive_utc())),
                    plaid_items::updated_at.eq(Utc::now().naive_utc()),
                ))
                .execute(conn)?;

            let item = plaid_items::table
                .filter(plaid_items::id.eq(&item_id))
                .select(PlaidItem::as_select())
                .first(conn)?;
            Ok((item, new_count))
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(PlaidSyncResponse {
        item: PlaidItemResponse::from(updated_item),
        new_transaction_count: new_count,
        updated_balance: balance,
    }))
}

/// Unlink an institution. This does not touch the linked account's balance
/// or delete previously-synced transactions.
#[utoipa::path(
    delete,
    path = "/api/plaid/items/{id}",
    tag = "plaid",
    params(("id" = String, Path, description = "Plaid item id")),
    responses(
        (status = 204, description = "Unlinked"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/plaid/items/{id}")]
pub async fn delete_item(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let deleted = web::block(move || -> AppResult<usize> {
        let mut conn = pool.get()?;
        let n = diesel::delete(
            plaid_items::table
                .filter(plaid_items::id.eq(&id))
                .filter(plaid_items::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Plaid item not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}
