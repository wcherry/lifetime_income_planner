use actix_web::{delete, get, post, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{IncomeRequest, IncomeResponse, IncomeSource, NewIncomeSource};
use crate::schema::income_sources;

/// List the authenticated user's income sources (newest first).
#[utoipa::path(
    get,
    path = "/api/income",
    tag = "income",
    responses(
        (status = 200, description = "The user's income sources", body = [IncomeResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/income")]
pub async fn list_income(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<IncomeSource>> {
        let mut conn = pool.get()?;
        let rows = income_sources::table
            .filter(income_sources::user_id.eq(&auth.user_id))
            .order(income_sources::created_at.desc())
            .select(IncomeSource::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<IncomeResponse> = rows.into_iter().map(IncomeResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Create an income source.
#[utoipa::path(
    post,
    path = "/api/income",
    tag = "income",
    request_body = IncomeRequest,
    responses(
        (status = 201, description = "Created", body = IncomeResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/income")]
pub async fn create_income(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<IncomeRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    validate_income(&payload)?;

    let row = build_new(Uuid::new_v4().to_string(), auth.user_id.clone(), &payload);
    let id = row.id.clone();

    let pool = pool.clone();
    let item = web::block(move || -> AppResult<IncomeSource> {
        let mut conn = pool.get()?;
        diesel::insert_into(income_sources::table)
            .values(&row)
            .execute(&mut conn)?;
        let item = income_sources::table
            .filter(income_sources::id.eq(&id))
            .select(IncomeSource::as_select())
            .first(&mut conn)?;
        Ok(item)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(IncomeResponse::from(item)))
}

/// Update an income source.
#[utoipa::path(
    put,
    path = "/api/income/{id}",
    tag = "income",
    params(("id" = String, Path, description = "Income source id")),
    request_body = IncomeRequest,
    responses(
        (status = 200, description = "Updated", body = IncomeResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/income/{id}")]
pub async fn update_income(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<IncomeRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let payload = body.into_inner();
    validate_income(&payload)?;

    let values = build_new(id.clone(), auth.user_id.clone(), &payload);
    let user_id = auth.user_id.clone();

    let pool = pool.clone();
    let item = web::block(move || -> AppResult<IncomeSource> {
        let mut conn = pool.get()?;
        let owned: i64 = income_sources::table
            .filter(income_sources::id.eq(&id))
            .filter(income_sources::user_id.eq(&user_id))
            .count()
            .get_result(&mut conn)?;
        if owned == 0 {
            return Err(AppError::NotFound("Income source not found".into()));
        }

        diesel::update(income_sources::table.filter(income_sources::id.eq(&id)))
            .set((
                income_sources::name.eq(&values.name),
                income_sources::income_type.eq(&values.income_type),
                income_sources::owner.eq(&values.owner),
                income_sources::amount.eq(values.amount),
                income_sources::frequency.eq(&values.frequency),
                income_sources::start_date.eq(values.start_date),
                income_sources::end_date.eq(values.end_date),
                income_sources::growth_rate.eq(values.growth_rate),
                income_sources::cola.eq(values.cola),
                income_sources::taxability.eq(&values.taxability),
                income_sources::notes.eq(&values.notes),
                income_sources::updated_at.eq(values.updated_at),
            ))
            .execute(&mut conn)?;

        let item = income_sources::table
            .filter(income_sources::id.eq(&id))
            .select(IncomeSource::as_select())
            .first(&mut conn)?;
        Ok(item)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(IncomeResponse::from(item)))
}

/// Delete an income source.
#[utoipa::path(
    delete,
    path = "/api/income/{id}",
    tag = "income",
    params(("id" = String, Path, description = "Income source id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/income/{id}")]
pub async fn delete_income(
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
            income_sources::table
                .filter(income_sources::id.eq(&id))
                .filter(income_sources::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Income source not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// --- helpers ---

fn build_new(id: String, user_id: String, req: &IncomeRequest) -> NewIncomeSource {
    NewIncomeSource {
        id,
        user_id,
        name: req.name.trim().to_string(),
        income_type: req.income_type.as_str().to_string(),
        owner: req.owner.as_str().to_string(),
        amount: req.amount,
        frequency: req.frequency.as_str().to_string(),
        start_date: req.start_date,
        end_date: req.end_date,
        growth_rate: req.growth_rate,
        cola: req.cola,
        taxability: req.taxability.as_str().to_string(),
        notes: req
            .notes
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        updated_at: Utc::now().naive_utc(),
    }
}

fn validate_income(req: &IncomeRequest) -> AppResult<()> {
    req.validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;
    if let Some(end) = req.end_date {
        if end < req.start_date {
            return Err(AppError::BadRequest(
                "End date must be on or after the start date".into(),
            ));
        }
    }
    Ok(())
}
