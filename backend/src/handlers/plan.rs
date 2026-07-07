use actix_web::{delete, get, post, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{
    Account, Assumptions, IncomeSource, LifeEvent, NewPlan, Plan, PlanResponse, PlanSnapshot,
    Profile, SavePlanRequest, SpendingItem,
};
use crate::schema::{
    accounts, assumptions, income_sources, life_events, plans, profiles, spending_items,
};

/// List the authenticated user's saved plans (newest first).
#[utoipa::path(
    get,
    path = "/api/plans",
    tag = "plans",
    responses(
        (status = 200, description = "The user's saved plans", body = [PlanResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/plans")]
pub async fn list_plans(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<Plan>> {
        let mut conn = pool.get()?;
        let rows = plans::table
            .filter(plans::user_id.eq(&auth.user_id))
            .order(plans::created_at.desc())
            .select(Plan::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<PlanResponse> = rows.iter().map(PlanResponse::from_row).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Save the user's current working set (profile, assumptions, accounts, income,
/// spending, life events) as a new named plan.
#[utoipa::path(
    post,
    path = "/api/plans",
    tag = "plans",
    request_body = SavePlanRequest,
    responses(
        (status = 201, description = "Plan saved", body = PlanResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plans")]
pub async fn save_plan(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<SavePlanRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let user_id = auth.user_id.clone();
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Plan name is required".into()));
    }

    let pool = pool.clone();
    let plan = web::block(move || -> AppResult<Plan> {
        let mut conn = pool.get()?;

        // Capture the current working set.
        let profile = profiles::table
            .filter(profiles::user_id.eq(&user_id))
            .select(Profile::as_select())
            .first(&mut conn)
            .optional()?;
        let assumptions_row = assumptions::table
            .filter(assumptions::user_id.eq(&user_id))
            .select(Assumptions::as_select())
            .first(&mut conn)
            .optional()?;
        let account_rows = accounts::table
            .filter(accounts::user_id.eq(&user_id))
            .select(Account::as_select())
            .load(&mut conn)?;
        let income_rows = income_sources::table
            .filter(income_sources::user_id.eq(&user_id))
            .select(IncomeSource::as_select())
            .load(&mut conn)?;
        let spending_rows = spending_items::table
            .filter(spending_items::user_id.eq(&user_id))
            .select(SpendingItem::as_select())
            .load(&mut conn)?;
        let life_event_rows = life_events::table
            .filter(life_events::user_id.eq(&user_id))
            .select(LifeEvent::as_select())
            .load(&mut conn)?;

        let snapshot = PlanSnapshot::capture(
            profile.as_ref(),
            assumptions_row.as_ref(),
            &account_rows,
            &income_rows,
            &spending_rows,
            &life_event_rows,
        );
        let snapshot_json = serde_json::to_string(&snapshot)
            .map_err(|e| AppError::Internal(format!("failed to serialize snapshot: {e}")))?;

        let id = Uuid::new_v4().to_string();
        let new_plan = NewPlan {
            id: id.clone(),
            user_id: user_id.clone(),
            name,
            snapshot: snapshot_json,
            updated_at: Utc::now().naive_utc(),
        };
        diesel::insert_into(plans::table)
            .values(&new_plan)
            .execute(&mut conn)?;

        let plan = plans::table
            .filter(plans::id.eq(&id))
            .select(Plan::as_select())
            .first(&mut conn)?;
        Ok(plan)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(PlanResponse::from_row(&plan)))
}

/// Rename a saved plan.
#[utoipa::path(
    put,
    path = "/api/plans/{id}",
    tag = "plans",
    params(("id" = String, Path, description = "Plan id")),
    request_body = SavePlanRequest,
    responses(
        (status = 200, description = "Plan renamed", body = PlanResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/plans/{id}")]
pub async fn rename_plan(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<SavePlanRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Plan name is required".into()));
    }

    let pool = pool.clone();
    let plan = web::block(move || -> AppResult<Plan> {
        let mut conn = pool.get()?;
        let updated = diesel::update(
            plans::table
                .filter(plans::id.eq(&id))
                .filter(plans::user_id.eq(&user_id)),
        )
        .set((
            plans::name.eq(&name),
            plans::updated_at.eq(Utc::now().naive_utc()),
        ))
        .execute(&mut conn)?;
        if updated == 0 {
            return Err(AppError::NotFound("Plan not found".into()));
        }
        let plan = plans::table
            .filter(plans::id.eq(&id))
            .select(Plan::as_select())
            .first(&mut conn)?;
        Ok(plan)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(PlanResponse::from_row(&plan)))
}

/// Load a saved plan, replacing the current working set with its contents.
///
/// This is destructive to the live working set: existing accounts, income,
/// spending, and life events are cleared and the plan's are restored in one
/// transaction, along with the plan's profile and assumptions.
#[utoipa::path(
    post,
    path = "/api/plans/{id}/load",
    tag = "plans",
    params(("id" = String, Path, description = "Plan id")),
    responses(
        (status = 200, description = "Plan loaded into the working set", body = PlanResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plans/{id}/load")]
pub async fn load_plan(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let user_id = auth.user_id.clone();

    let pool = pool.clone();
    let plan = web::block(move || -> AppResult<Plan> {
        let mut conn = pool.get()?;

        let plan = plans::table
            .filter(plans::id.eq(&id))
            .filter(plans::user_id.eq(&user_id))
            .select(Plan::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Plan not found".into()))?;

        let snapshot: PlanSnapshot = serde_json::from_str(&plan.snapshot)
            .map_err(|e| AppError::Internal(format!("corrupt plan snapshot: {e}")))?;

        let now = Utc::now().naive_utc();

        conn.transaction::<(), AppError, _>(|conn| {
            // Clear the current working set for the collection resources.
            diesel::delete(accounts::table.filter(accounts::user_id.eq(&user_id)))
                .execute(conn)?;
            diesel::delete(income_sources::table.filter(income_sources::user_id.eq(&user_id)))
                .execute(conn)?;
            diesel::delete(spending_items::table.filter(spending_items::user_id.eq(&user_id)))
                .execute(conn)?;
            diesel::delete(life_events::table.filter(life_events::user_id.eq(&user_id)))
                .execute(conn)?;
            diesel::delete(profiles::table.filter(profiles::user_id.eq(&user_id)))
                .execute(conn)?;
            diesel::delete(assumptions::table.filter(assumptions::user_id.eq(&user_id)))
                .execute(conn)?;

            // Restore the snapshot with fresh ids.
            if let Some(p) = &snapshot.profile {
                let row = p.into_changeset(Uuid::new_v4().to_string(), user_id.clone(), now);
                diesel::insert_into(profiles::table).values(&row).execute(conn)?;
            }
            if let Some(a) = &snapshot.assumptions {
                let row = a.into_new(Uuid::new_v4().to_string(), user_id.clone(), now);
                diesel::insert_into(assumptions::table).values(&row).execute(conn)?;
            }
            for a in &snapshot.accounts {
                let row = a.into_new(Uuid::new_v4().to_string(), user_id.clone(), now);
                diesel::insert_into(accounts::table).values(&row).execute(conn)?;
            }
            for i in &snapshot.income {
                let row = i.into_new(Uuid::new_v4().to_string(), user_id.clone(), now);
                diesel::insert_into(income_sources::table).values(&row).execute(conn)?;
            }
            for s in &snapshot.spending {
                let row = s.into_new(Uuid::new_v4().to_string(), user_id.clone(), now);
                diesel::insert_into(spending_items::table).values(&row).execute(conn)?;
            }
            for e in &snapshot.life_events {
                let row = e.into_new(Uuid::new_v4().to_string(), user_id.clone(), now);
                diesel::insert_into(life_events::table).values(&row).execute(conn)?;
            }
            Ok(())
        })?;

        Ok(plan)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(PlanResponse::from_row(&plan)))
}

/// Delete a saved plan. Does not affect the live working set.
#[utoipa::path(
    delete,
    path = "/api/plans/{id}",
    tag = "plans",
    params(("id" = String, Path, description = "Plan id")),
    responses(
        (status = 204, description = "Plan deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/plans/{id}")]
pub async fn delete_plan(
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
            plans::table
                .filter(plans::id.eq(&id))
                .filter(plans::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Plan not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}
