use actix_web::{delete, get, post, put, web, HttpResponse};
use chrono::{Datelike, Utc};
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::handlers::projection::loaded_projection_data_from_snapshot;
use crate::models::aca::load_aca_tables;
use crate::models::irmaa::load_irmaa_tables;
use crate::models::tax::load_tax_tables;
use crate::models::{
    Account, Assumptions, ClonePlanRequest, CompareScenariosRequest, IncomeSource, LifeEvent,
    NewPlan, NewPlanSnapshotVersion, Plan, PlanResponse, PlanSnapshot, PlanSnapshotVersion,
    PlanVersionResponse, Profile, SavePlanRequest, ScenarioComparison, SpendingItem,
};
use crate::projection::run_projection;
use crate::schema::{
    accounts, assumptions, income_sources, life_events, plan_snapshots, plans, profiles,
    spending_items,
};
use diesel::sqlite::SqliteConnection;

/// Capture the user's current working set (profile, assumptions, accounts,
/// income, spending, life events) as a serialized [`PlanSnapshot`]. Shared by
/// `save_plan` (a brand-new plan) and `update_plan_snapshot` (Phase 4,
/// feature 7 — refreshing an existing scenario's data).
fn capture_current_snapshot_json(conn: &mut SqliteConnection, user_id: &str) -> AppResult<String> {
    let profile = profiles::table
        .filter(profiles::user_id.eq(user_id))
        .select(Profile::as_select())
        .first(conn)
        .optional()?;
    let assumptions_row = assumptions::table
        .filter(assumptions::user_id.eq(user_id))
        .select(Assumptions::as_select())
        .first(conn)
        .optional()?;
    let account_rows = accounts::table
        .filter(accounts::user_id.eq(user_id))
        .select(Account::as_select())
        .load(conn)?;
    let income_rows = income_sources::table
        .filter(income_sources::user_id.eq(user_id))
        .select(IncomeSource::as_select())
        .load(conn)?;
    let spending_rows = spending_items::table
        .filter(spending_items::user_id.eq(user_id))
        .select(SpendingItem::as_select())
        .load(conn)?;
    let life_event_rows = life_events::table
        .filter(life_events::user_id.eq(user_id))
        .select(LifeEvent::as_select())
        .load(conn)?;

    let snapshot = PlanSnapshot::capture(
        profile.as_ref(),
        assumptions_row.as_ref(),
        &account_rows,
        &income_rows,
        &spending_rows,
        &life_event_rows,
    );
    serde_json::to_string(&snapshot)
        .map_err(|e| AppError::Internal(format!("failed to serialize snapshot: {e}")))
}

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

/// Compare two or more saved plans side by side (roadmap Phase 4, feature 2):
/// runs each plan's saved snapshot through the same projection engine as the
/// live working set — without touching it — and returns the headline figures
/// for each, so a user can weigh strategies (taxes, estate, ACA subsidies,
/// RMDs, spending, depletion age) against each other.
#[utoipa::path(
    post,
    path = "/api/plans/compare",
    tag = "plans",
    request_body = CompareScenariosRequest,
    responses(
        (status = 200, description = "Comparison across the requested scenarios", body = [ScenarioComparison]),
        (status = 400, description = "Validation error, or a scenario has no profile saved"),
        (status = 404, description = "One of the requested plans was not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plans/compare")]
pub async fn compare_plans(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<CompareScenariosRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let user_id = auth.user_id.clone();
    let plan_ids = payload.plan_ids;
    let pool = pool.clone();
    let (rows, tax_tables, aca_tables, irmaa_tables) = web::block(move || -> AppResult<_> {
        let mut conn = pool.get()?;
        let mut rows = Vec::with_capacity(plan_ids.len());
        for id in &plan_ids {
            let plan = plans::table
                .filter(plans::id.eq(id))
                .filter(plans::user_id.eq(&user_id))
                .select(Plan::as_select())
                .first::<Plan>(&mut conn)
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Plan {id} not found")))?;
            rows.push(plan);
        }
        let tax_tables = load_tax_tables(&mut conn)?;
        let aca_tables = load_aca_tables(&mut conn)?;
        let irmaa_tables = load_irmaa_tables(&mut conn)?;
        Ok((rows, tax_tables, aca_tables, irmaa_tables))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let current_year = Utc::now().year();
    let mut results = Vec::with_capacity(rows.len());
    for plan in rows {
        let snapshot: PlanSnapshot = serde_json::from_str(&plan.snapshot)
            .map_err(|e| AppError::Internal(format!("corrupt plan snapshot: {e}")))?;
        let data = loaded_projection_data_from_snapshot(
            &snapshot,
            tax_tables.clone(),
            aca_tables.clone(),
            irmaa_tables.clone(),
        )
        .map_err(|e| match e {
            AppError::BadRequest(msg) => AppError::BadRequest(format!("\"{}\" {msg}", plan.name)),
            other => other,
        })?;
        let birth_year = data.profile.date_of_birth.year();
        let inputs = data.inputs(current_year);
        let projection = run_projection(&inputs);
        let depletion_age = projection.summary.depletion_year.map(|y| y - birth_year);
        results.push(ScenarioComparison {
            plan_id: plan.id,
            plan_name: plan.name,
            summary: projection.summary,
            depletion_age,
        });
    }

    Ok(HttpResponse::Ok().json(results))
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
        let snapshot_json = capture_current_snapshot_json(&mut conn, &user_id)?;

        let id = Uuid::new_v4().to_string();
        let new_plan = NewPlan {
            id: id.clone(),
            user_id: user_id.clone(),
            name,
            snapshot: snapshot_json,
            updated_at: Utc::now().naive_utc(),
            parent_plan_id: None,
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

/// Clone a saved plan (roadmap Phase 4, feature 4): duplicates its snapshot
/// into a new plan, so the user can branch off an existing scenario and
/// iterate — via the normal load/edit/save flow — without disturbing the
/// original. The new plan records `parent_plan_id` for lineage.
#[utoipa::path(
    post,
    path = "/api/plans/{id}/clone",
    tag = "plans",
    params(("id" = String, Path, description = "Plan id to clone")),
    request_body = ClonePlanRequest,
    responses(
        (status = 201, description = "Plan cloned", body = PlanResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plans/{id}/clone")]
pub async fn clone_plan(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
    body: web::Json<ClonePlanRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let source_id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let plan = web::block(move || -> AppResult<Plan> {
        let mut conn = pool.get()?;
        let source = plans::table
            .filter(plans::id.eq(&source_id))
            .filter(plans::user_id.eq(&user_id))
            .select(Plan::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Plan not found".into()))?;

        let name = payload
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("{} copy", source.name));
        let name = name.chars().take(120).collect::<String>();

        let id = Uuid::new_v4().to_string();
        let new_plan = NewPlan {
            id: id.clone(),
            user_id: source.user_id.clone(),
            name,
            snapshot: source.snapshot.clone(),
            updated_at: Utc::now().naive_utc(),
            parent_plan_id: Some(source.id.clone()),
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

/// Refresh a saved plan with the user's current working set (roadmap Phase 4,
/// feature 7): archives the plan's displaced snapshot into its version
/// history first, so a scenario accumulates a timeline as it evolves instead
/// of silently overwriting what came before. The archived version's
/// timestamp is the plan's previous `updated_at` — when that snapshot
/// actually became current — not the moment it was archived.
#[utoipa::path(
    post,
    path = "/api/plans/{id}/versions",
    tag = "plans",
    params(("id" = String, Path, description = "Plan id")),
    responses(
        (status = 200, description = "Plan refreshed with the current working set", body = PlanResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plans/{id}/versions")]
pub async fn update_plan_snapshot(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let plan = web::block(move || -> AppResult<Plan> {
        let mut conn = pool.get()?;

        let existing = plans::table
            .filter(plans::id.eq(&id))
            .filter(plans::user_id.eq(&user_id))
            .select(Plan::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Plan not found".into()))?;

        let snapshot_json = capture_current_snapshot_json(&mut conn, &user_id)?;

        conn.transaction::<(), AppError, _>(|conn| {
            let archived = NewPlanSnapshotVersion {
                id: Uuid::new_v4().to_string(),
                plan_id: existing.id.clone(),
                user_id: user_id.clone(),
                snapshot: existing.snapshot.clone(),
                created_at: existing.updated_at,
            };
            diesel::insert_into(plan_snapshots::table)
                .values(&archived)
                .execute(conn)?;

            diesel::update(plans::table.filter(plans::id.eq(&existing.id)))
                .set((
                    plans::snapshot.eq(&snapshot_json),
                    plans::updated_at.eq(Utc::now().naive_utc()),
                ))
                .execute(conn)?;
            Ok(())
        })?;

        let plan = plans::table
            .filter(plans::id.eq(&existing.id))
            .select(Plan::as_select())
            .first(&mut conn)?;
        Ok(plan)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(PlanResponse::from_row(&plan)))
}

/// List a plan's historical versions (roadmap Phase 4, feature 7), newest
/// first. Does not include the plan's current snapshot — that's already
/// visible via `GET /plans`.
#[utoipa::path(
    get,
    path = "/api/plans/{id}/versions",
    tag = "plans",
    params(("id" = String, Path, description = "Plan id")),
    responses(
        (status = 200, description = "The plan's historical versions, newest first", body = [PlanVersionResponse]),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/plans/{id}/versions")]
pub async fn list_plan_versions(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let versions = web::block(move || -> AppResult<Vec<PlanSnapshotVersion>> {
        let mut conn = pool.get()?;

        let owned = plans::table
            .filter(plans::id.eq(&id))
            .filter(plans::user_id.eq(&user_id))
            .select(Plan::as_select())
            .first(&mut conn)
            .optional()?
            .is_some();
        if !owned {
            return Err(AppError::NotFound("Plan not found".into()));
        }

        let rows = plan_snapshots::table
            .filter(plan_snapshots::plan_id.eq(&id))
            .filter(plan_snapshots::user_id.eq(&user_id))
            .order(plan_snapshots::created_at.desc())
            .select(PlanSnapshotVersion::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<PlanVersionResponse> = versions.iter().map(PlanVersionResponse::from_row).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Restore one of a plan's historical versions (roadmap Phase 4, feature 7):
/// makes that version the plan's current snapshot again. The snapshot it
/// displaces is archived first, so restoring never loses data — it just adds
/// another entry to the timeline.
#[utoipa::path(
    post,
    path = "/api/plans/{id}/versions/{version_id}/restore",
    tag = "plans",
    params(
        ("id" = String, Path, description = "Plan id"),
        ("version_id" = String, Path, description = "Historical version id to restore"),
    ),
    responses(
        (status = 200, description = "Plan restored to the historical version", body = PlanResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/plans/{id}/versions/{version_id}/restore")]
pub async fn restore_plan_version(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<(String, String)>,
) -> AppResult<HttpResponse> {
    let (id, version_id) = path.into_inner();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let plan = web::block(move || -> AppResult<Plan> {
        let mut conn = pool.get()?;

        let existing = plans::table
            .filter(plans::id.eq(&id))
            .filter(plans::user_id.eq(&user_id))
            .select(Plan::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Plan not found".into()))?;

        let version = plan_snapshots::table
            .filter(plan_snapshots::id.eq(&version_id))
            .filter(plan_snapshots::plan_id.eq(&existing.id))
            .filter(plan_snapshots::user_id.eq(&user_id))
            .select(PlanSnapshotVersion::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound("Version not found".into()))?;

        conn.transaction::<(), AppError, _>(|conn| {
            let archived = NewPlanSnapshotVersion {
                id: Uuid::new_v4().to_string(),
                plan_id: existing.id.clone(),
                user_id: user_id.clone(),
                snapshot: existing.snapshot.clone(),
                created_at: existing.updated_at,
            };
            diesel::insert_into(plan_snapshots::table)
                .values(&archived)
                .execute(conn)?;

            diesel::update(plans::table.filter(plans::id.eq(&existing.id)))
                .set((
                    plans::snapshot.eq(&version.snapshot),
                    plans::updated_at.eq(Utc::now().naive_utc()),
                ))
                .execute(conn)?;
            Ok(())
        })?;

        let plan = plans::table
            .filter(plans::id.eq(&existing.id))
            .select(Plan::as_select())
            .first(&mut conn)?;
        Ok(plan)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(PlanResponse::from_row(&plan)))
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
