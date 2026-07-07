use actix_web::{delete, get, post, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{Account, AccountCategory, AccountRequest, AccountResponse, NewAccount};
use crate::schema::accounts;

/// List all of the authenticated user's accounts (newest first).
#[utoipa::path(
    get,
    path = "/api/accounts",
    tag = "accounts",
    responses(
        (status = 200, description = "The user's accounts", body = [AccountResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/accounts")]
pub async fn list_accounts(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<Account>> {
        let mut conn = pool.get()?;
        let rows = accounts::table
            .filter(accounts::user_id.eq(&auth.user_id))
            .order(accounts::created_at.desc())
            .select(Account::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<AccountResponse> = rows.into_iter().map(AccountResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Create a new account.
#[utoipa::path(
    post,
    path = "/api/accounts",
    tag = "accounts",
    request_body = AccountRequest,
    responses(
        (status = 201, description = "Account created", body = AccountResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/accounts")]
pub async fn create_account(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<AccountRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    validate_account(&payload)?;

    let new_account = build_new_account(Uuid::new_v4().to_string(), auth.user_id.clone(), &payload);
    let id = new_account.id.clone();

    let pool = pool.clone();
    let account = web::block(move || -> AppResult<Account> {
        let mut conn = pool.get()?;
        diesel::insert_into(accounts::table)
            .values(&new_account)
            .execute(&mut conn)?;
        let account = accounts::table
            .filter(accounts::id.eq(&id))
            .select(Account::as_select())
            .first(&mut conn)?;
        Ok(account)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(AccountResponse::from(account)))
}

/// Fetch a single account by id.
#[utoipa::path(
    get,
    path = "/api/accounts/{id}",
    tag = "accounts",
    params(("id" = String, Path, description = "Account id")),
    responses(
        (status = 200, description = "The account", body = AccountResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/accounts/{id}")]
pub async fn get_account(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let account = load_owned_account(&pool, &auth.user_id, &id).await?;
    Ok(HttpResponse::Ok().json(AccountResponse::from(account)))
}

/// Update an existing account.
#[utoipa::path(
    put,
    path = "/api/accounts/{id}",
    tag = "accounts",
    params(("id" = String, Path, description = "Account id")),
    request_body = AccountRequest,
    responses(
        (status = 200, description = "Account updated", body = AccountResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/accounts/{id}")]
pub async fn update_account(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<AccountRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let payload = body.into_inner();
    validate_account(&payload)?;

    let values = build_new_account(id.clone(), auth.user_id.clone(), &payload);
    let user_id = auth.user_id.clone();

    let pool = pool.clone();
    let account = web::block(move || -> AppResult<Account> {
        let mut conn = pool.get()?;

        // Ensure the account exists and belongs to the caller before updating.
        let owned: i64 = accounts::table
            .filter(accounts::id.eq(&id))
            .filter(accounts::user_id.eq(&user_id))
            .count()
            .get_result(&mut conn)?;
        if owned == 0 {
            return Err(AppError::NotFound("Account not found".into()));
        }

        diesel::update(accounts::table.filter(accounts::id.eq(&id)))
            .set((
                accounts::name.eq(&values.name),
                accounts::category.eq(&values.category),
                accounts::account_type.eq(&values.account_type),
                accounts::owner.eq(&values.owner),
                accounts::current_balance.eq(values.current_balance),
                accounts::expected_roi.eq(values.expected_roi),
                accounts::dividend_yield.eq(values.dividend_yield),
                accounts::cost_basis.eq(values.cost_basis),
                accounts::allocation_stock_pct.eq(values.allocation_stock_pct),
                accounts::allocation_bond_pct.eq(values.allocation_bond_pct),
                accounts::allocation_cash_pct.eq(values.allocation_cash_pct),
                accounts::withdrawal_restrictions.eq(&values.withdrawal_restrictions),
                accounts::updated_at.eq(values.updated_at),
            ))
            .execute(&mut conn)?;

        let account = accounts::table
            .filter(accounts::id.eq(&id))
            .select(Account::as_select())
            .first(&mut conn)?;
        Ok(account)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(AccountResponse::from(account)))
}

/// Delete an account.
#[utoipa::path(
    delete,
    path = "/api/accounts/{id}",
    tag = "accounts",
    params(("id" = String, Path, description = "Account id")),
    responses(
        (status = 204, description = "Account deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/accounts/{id}")]
pub async fn delete_account(
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
            accounts::table
                .filter(accounts::id.eq(&id))
                .filter(accounts::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Account not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// --- helpers ---

async fn load_owned_account(
    pool: &web::Data<DbPool>,
    user_id: &str,
    id: &str,
) -> AppResult<Account> {
    let pool = pool.clone();
    let user_id = user_id.to_string();
    let id = id.to_string();
    let account = web::block(move || -> AppResult<Option<Account>> {
        let mut conn = pool.get()?;
        let account = accounts::table
            .filter(accounts::id.eq(&id))
            .filter(accounts::user_id.eq(&user_id))
            .select(Account::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(account)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    account.ok_or_else(|| AppError::NotFound("Account not found".into()))
}

/// Build a persistable row from a request. Cost basis is only kept for taxable
/// accounts, mirroring how spouse data is scoped on the profile.
fn build_new_account(id: String, user_id: String, req: &AccountRequest) -> NewAccount {
    let cost_basis = if req.category == AccountCategory::Taxable {
        req.cost_basis
    } else {
        None
    };
    NewAccount {
        id,
        user_id,
        name: req.name.trim().to_string(),
        category: req.category.as_str().to_string(),
        account_type: req.account_type.as_str().to_string(),
        owner: req.owner.as_str().to_string(),
        current_balance: req.current_balance,
        expected_roi: req.expected_roi,
        dividend_yield: req.dividend_yield,
        cost_basis,
        allocation_stock_pct: req.allocation_stock_pct,
        allocation_bond_pct: req.allocation_bond_pct,
        allocation_cash_pct: req.allocation_cash_pct,
        withdrawal_restrictions: req
            .withdrawal_restrictions
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        updated_at: Utc::now().naive_utc(),
    }
}

/// Per-field validation plus the cross-field allocation rule.
fn validate_account(req: &AccountRequest) -> AppResult<()> {
    req.validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    // Allocation is all-or-nothing and must sum to 100 when supplied.
    let alloc = [
        req.allocation_stock_pct,
        req.allocation_bond_pct,
        req.allocation_cash_pct,
    ];
    let provided = alloc.iter().filter(|v| v.is_some()).count();
    if provided > 0 {
        if provided != 3 {
            return Err(AppError::BadRequest(
                "Provide all three allocation percentages (stock, bond, cash) or none".into(),
            ));
        }
        let sum: i32 = alloc.iter().map(|v| v.unwrap_or(0)).sum();
        if sum != 100 {
            return Err(AppError::BadRequest(
                "Allocation percentages must sum to 100".into(),
            ));
        }
    }

    Ok(())
}
