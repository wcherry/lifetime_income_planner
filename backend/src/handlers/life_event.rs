use actix_web::{delete, get, post, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{EventRecurrence, LifeEvent, LifeEventRequest, LifeEventResponse, NewLifeEvent};
use crate::schema::life_events;

/// List the authenticated user's life events (soonest event first).
#[utoipa::path(
    get,
    path = "/api/life-events",
    tag = "life_events",
    responses(
        (status = 200, description = "The user's life events", body = [LifeEventResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/life-events")]
pub async fn list_life_events(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<LifeEvent>> {
        let mut conn = pool.get()?;
        let rows = life_events::table
            .filter(life_events::user_id.eq(&auth.user_id))
            .order(life_events::event_date.asc())
            .select(LifeEvent::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<LifeEventResponse> = rows.into_iter().map(LifeEventResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Create a life event.
#[utoipa::path(
    post,
    path = "/api/life-events",
    tag = "life_events",
    request_body = LifeEventRequest,
    responses(
        (status = 201, description = "Created", body = LifeEventResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/life-events")]
pub async fn create_life_event(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<LifeEventRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    validate_life_event(&payload)?;

    let row = build_new(Uuid::new_v4().to_string(), auth.user_id.clone(), &payload);
    let id = row.id.clone();

    let pool = pool.clone();
    let item = web::block(move || -> AppResult<LifeEvent> {
        let mut conn = pool.get()?;
        diesel::insert_into(life_events::table)
            .values(&row)
            .execute(&mut conn)?;
        let item = life_events::table
            .filter(life_events::id.eq(&id))
            .select(LifeEvent::as_select())
            .first(&mut conn)?;
        Ok(item)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(LifeEventResponse::from(item)))
}

/// Update a life event.
#[utoipa::path(
    put,
    path = "/api/life-events/{id}",
    tag = "life_events",
    params(("id" = String, Path, description = "Life event id")),
    request_body = LifeEventRequest,
    responses(
        (status = 200, description = "Updated", body = LifeEventResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/life-events/{id}")]
pub async fn update_life_event(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<LifeEventRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let payload = body.into_inner();
    validate_life_event(&payload)?;

    let values = build_new(id.clone(), auth.user_id.clone(), &payload);
    let user_id = auth.user_id.clone();

    let pool = pool.clone();
    let item = web::block(move || -> AppResult<LifeEvent> {
        let mut conn = pool.get()?;
        let owned: i64 = life_events::table
            .filter(life_events::id.eq(&id))
            .filter(life_events::user_id.eq(&user_id))
            .count()
            .get_result(&mut conn)?;
        if owned == 0 {
            return Err(AppError::NotFound("Life event not found".into()));
        }

        diesel::update(life_events::table.filter(life_events::id.eq(&id)))
            .set((
                life_events::name.eq(&values.name),
                life_events::event_type.eq(&values.event_type),
                life_events::event_date.eq(values.event_date),
                life_events::direction.eq(&values.direction),
                life_events::amount.eq(values.amount),
                life_events::taxable.eq(values.taxable),
                life_events::inflation_adjusted.eq(values.inflation_adjusted),
                life_events::recurrence.eq(&values.recurrence),
                life_events::end_date.eq(values.end_date),
                life_events::notes.eq(&values.notes),
                life_events::updated_at.eq(values.updated_at),
            ))
            .execute(&mut conn)?;

        let item = life_events::table
            .filter(life_events::id.eq(&id))
            .select(LifeEvent::as_select())
            .first(&mut conn)?;
        Ok(item)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(LifeEventResponse::from(item)))
}

/// Delete a life event.
#[utoipa::path(
    delete,
    path = "/api/life-events/{id}",
    tag = "life_events",
    params(("id" = String, Path, description = "Life event id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/life-events/{id}")]
pub async fn delete_life_event(
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
            life_events::table
                .filter(life_events::id.eq(&id))
                .filter(life_events::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Life event not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// --- helpers ---

fn build_new(id: String, user_id: String, req: &LifeEventRequest) -> NewLifeEvent {
    NewLifeEvent {
        id,
        user_id,
        name: req.name.trim().to_string(),
        event_type: req.event_type.as_str().to_string(),
        event_date: req.event_date,
        direction: req.direction.as_str().to_string(),
        amount: req.amount,
        taxable: req.taxable,
        inflation_adjusted: req.inflation_adjusted,
        recurrence: req.recurrence.as_str().to_string(),
        // An end date only applies to recurring events.
        end_date: if req.recurrence == EventRecurrence::OneTime {
            None
        } else {
            req.end_date
        },
        notes: req
            .notes
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        updated_at: Utc::now().naive_utc(),
    }
}

fn validate_life_event(req: &LifeEventRequest) -> AppResult<()> {
    req.validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;
    if req.recurrence != EventRecurrence::OneTime {
        if let Some(end) = req.end_date {
            if end < req.event_date {
                return Err(AppError::BadRequest(
                    "End date must be on or after the event date".into(),
                ));
            }
        }
    }
    Ok(())
}
