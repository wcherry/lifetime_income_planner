use actix_web::{delete, get, post, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{NewSpendingItem, SpendingItem, SpendingRequest, SpendingResponse};
use crate::schema::spending_items;

/// List the authenticated user's spending items (newest first).
#[utoipa::path(
    get,
    path = "/api/spending",
    tag = "spending",
    responses(
        (status = 200, description = "The user's spending items", body = [SpendingResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/spending")]
pub async fn list_spending(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<SpendingItem>> {
        let mut conn = pool.get()?;
        let rows = spending_items::table
            .filter(spending_items::user_id.eq(&auth.user_id))
            .order(spending_items::created_at.desc())
            .select(SpendingItem::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<SpendingResponse> = rows.into_iter().map(SpendingResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Create a spending item.
#[utoipa::path(
    post,
    path = "/api/spending",
    tag = "spending",
    request_body = SpendingRequest,
    responses(
        (status = 201, description = "Created", body = SpendingResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/spending")]
pub async fn create_spending(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<SpendingRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    validate_spending(&payload)?;

    let row = build_new(Uuid::new_v4().to_string(), auth.user_id.clone(), &payload);
    let id = row.id.clone();

    let pool = pool.clone();
    let item = web::block(move || -> AppResult<SpendingItem> {
        let mut conn = pool.get()?;
        diesel::insert_into(spending_items::table)
            .values(&row)
            .execute(&mut conn)?;
        let item = spending_items::table
            .filter(spending_items::id.eq(&id))
            .select(SpendingItem::as_select())
            .first(&mut conn)?;
        Ok(item)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(SpendingResponse::from(item)))
}

/// Update a spending item.
#[utoipa::path(
    put,
    path = "/api/spending/{id}",
    tag = "spending",
    params(("id" = String, Path, description = "Spending item id")),
    request_body = SpendingRequest,
    responses(
        (status = 200, description = "Updated", body = SpendingResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/spending/{id}")]
pub async fn update_spending(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<SpendingRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let payload = body.into_inner();
    validate_spending(&payload)?;

    let values = build_new(id.clone(), auth.user_id.clone(), &payload);
    let user_id = auth.user_id.clone();

    let pool = pool.clone();
    let item = web::block(move || -> AppResult<SpendingItem> {
        let mut conn = pool.get()?;
        let owned: i64 = spending_items::table
            .filter(spending_items::id.eq(&id))
            .filter(spending_items::user_id.eq(&user_id))
            .count()
            .get_result(&mut conn)?;
        if owned == 0 {
            return Err(AppError::NotFound("Spending item not found".into()));
        }

        diesel::update(spending_items::table.filter(spending_items::id.eq(&id)))
            .set((
                spending_items::name.eq(&values.name),
                spending_items::category.eq(&values.category),
                spending_items::amount.eq(values.amount),
                spending_items::frequency.eq(&values.frequency),
                spending_items::inflation_adjusted.eq(values.inflation_adjusted),
                spending_items::start_year.eq(values.start_year),
                spending_items::end_year.eq(values.end_year),
                spending_items::notes.eq(&values.notes),
                spending_items::updated_at.eq(values.updated_at),
            ))
            .execute(&mut conn)?;

        let item = spending_items::table
            .filter(spending_items::id.eq(&id))
            .select(SpendingItem::as_select())
            .first(&mut conn)?;
        Ok(item)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(SpendingResponse::from(item)))
}

/// Delete a spending item.
#[utoipa::path(
    delete,
    path = "/api/spending/{id}",
    tag = "spending",
    params(("id" = String, Path, description = "Spending item id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/spending/{id}")]
pub async fn delete_spending(
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
            spending_items::table
                .filter(spending_items::id.eq(&id))
                .filter(spending_items::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Spending item not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// --- helpers ---

fn build_new(id: String, user_id: String, req: &SpendingRequest) -> NewSpendingItem {
    NewSpendingItem {
        id,
        user_id,
        name: req.name.trim().to_string(),
        category: req.category.as_str().to_string(),
        amount: req.amount,
        frequency: req.frequency.as_str().to_string(),
        inflation_adjusted: req.inflation_adjusted,
        start_year: req.start_year,
        end_year: req.end_year,
        notes: req
            .notes
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        updated_at: Utc::now().naive_utc(),
    }
}

fn validate_spending(req: &SpendingRequest) -> AppResult<()> {
    req.validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;
    if let (Some(start), Some(end)) = (req.start_year, req.end_year) {
        if end < start {
            return Err(AppError::BadRequest(
                "End year must be on or after the start year".into(),
            ));
        }
    }
    Ok(())
}
