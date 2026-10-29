//! Task lists and reminders (roadmap Phase 5, feature 5): assembles this
//! quarter's and year's tasks from `crate::tasks::derive_tasks`, merges in
//! the user's completion state, and lets them check tasks off.

use std::collections::HashSet;

use actix_web::{delete, get, post, web, HttpResponse};
use chrono::{Datelike, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::projection::build_projection;
use crate::models::{NewTaskCompletion, TaskCompletion};
use crate::reconciliation;
use crate::schema::task_completions;
use crate::tasks::{derive_tasks, Task, TaskContext};

/// This quarter's and year's actionable tasks, with completion state merged
/// in from `task_completions`.
#[utoipa::path(
    get,
    path = "/api/tasks",
    tag = "tasks",
    responses(
        (status = 200, description = "Generated tasks for the current quarter and year", body = [Task]),
        (status = 400, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/tasks")]
pub async fn list_tasks(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let projection = build_projection(&pool, user_id.clone()).await?;

    let now = Utc::now();
    let current_year = now.year();
    let current_quarter = reconciliation::quarter_of_month(now.month());

    let current_year_projection = projection.annual.iter().find(|y| y.year == current_year);
    let current_quarter_projection = projection
        .quarterly
        .iter()
        .find(|q| q.year == current_year && q.quarter == current_quarter);

    let ctx = TaskContext {
        current_year,
        current_year_projection,
        current_quarter_projection,
    };
    let mut tasks = derive_tasks(&ctx);

    let pool_clone = pool.clone();
    let completed_keys: HashSet<String> = web::block(move || -> AppResult<Vec<String>> {
        let mut conn = pool_clone.get()?;
        let rows = task_completions::table
            .filter(task_completions::user_id.eq(&user_id))
            .select(task_completions::task_key)
            .load::<String>(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??
    .into_iter()
    .collect();

    for t in &mut tasks {
        t.completed = completed_keys.contains(&t.key);
    }

    Ok(HttpResponse::Ok().json(tasks))
}

/// Mark a generated task complete. Idempotent — completing an
/// already-completed task is a no-op rather than an error.
#[utoipa::path(
    post,
    path = "/api/tasks/{task_key}/complete",
    tag = "tasks",
    params(("task_key" = String, Path, description = "The task's stable key, as returned by GET /tasks")),
    responses(
        (status = 204, description = "Task marked complete"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/tasks/{task_key}/complete")]
pub async fn complete_task(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let task_key = path.into_inner();
    let pool = pool.clone();
    web::block(move || -> AppResult<()> {
        let mut conn = pool.get()?;
        let existing = task_completions::table
            .filter(task_completions::user_id.eq(&user_id))
            .filter(task_completions::task_key.eq(&task_key))
            .select(TaskCompletion::as_select())
            .first::<TaskCompletion>(&mut conn)
            .optional()?;
        if existing.is_none() {
            let new_row = NewTaskCompletion {
                id: Uuid::new_v4().to_string(),
                user_id,
                task_key,
                completed_at: Utc::now().naive_utc(),
            };
            diesel::insert_into(task_completions::table)
                .values(&new_row)
                .execute(&mut conn)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::NoContent().finish())
}

/// Clear a task's completion, e.g. if it was checked off by mistake.
/// Idempotent — clearing an already-incomplete task is a no-op.
#[utoipa::path(
    delete,
    path = "/api/tasks/{task_key}/complete",
    tag = "tasks",
    params(("task_key" = String, Path, description = "The task's stable key, as returned by GET /tasks")),
    responses(
        (status = 204, description = "Task completion cleared"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/tasks/{task_key}/complete")]
pub async fn uncomplete_task(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let task_key = path.into_inner();
    let pool = pool.clone();
    web::block(move || -> AppResult<()> {
        let mut conn = pool.get()?;
        diesel::delete(
            task_completions::table
                .filter(task_completions::user_id.eq(&user_id))
                .filter(task_completions::task_key.eq(&task_key)),
        )
        .execute(&mut conn)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::NoContent().finish())
}
