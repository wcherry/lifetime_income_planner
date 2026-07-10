//! Social Security statement import (roadmap Phase 6, feature 4).

use actix_web::{delete, get, post, web, HttpResponse};
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{
    ImportSocialSecurityEstimateRequest, NewSocialSecurityEstimate, SocialSecurityEstimate,
    SocialSecurityEstimateResponse,
};
use crate::schema::social_security_estimates;

/// Import an SSA statement's claiming-age estimates (roadmap Phase 6,
/// feature 4).
#[utoipa::path(
    post,
    path = "/api/social-security-estimates/import",
    tag = "social_security",
    request_body = ImportSocialSecurityEstimateRequest,
    responses(
        (status = 201, description = "Estimate imported", body = SocialSecurityEstimateResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/social-security-estimates/import")]
pub async fn import_estimate(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<ImportSocialSecurityEstimateRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    if payload.estimate_at_62.is_none()
        && payload.estimate_at_67.is_none()
        && payload.estimate_at_70.is_none()
    {
        return Err(AppError::BadRequest(
            "At least one claiming-age estimate is required".into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let new_estimate = NewSocialSecurityEstimate {
        id: id.clone(),
        user_id: auth.user_id.clone(),
        owner: payload.owner.as_str().to_string(),
        statement_date: payload.statement_date,
        estimate_at_62: payload.estimate_at_62,
        estimate_at_67: payload.estimate_at_67,
        estimate_at_70: payload.estimate_at_70,
        source: "import".to_string(),
    };

    let pool = pool.clone();
    let estimate = web::block(move || -> AppResult<SocialSecurityEstimate> {
        let mut conn = pool.get()?;
        diesel::insert_into(social_security_estimates::table)
            .values(&new_estimate)
            .execute(&mut conn)?;
        let estimate = social_security_estimates::table
            .filter(social_security_estimates::id.eq(&id))
            .select(SocialSecurityEstimate::as_select())
            .first(&mut conn)?;
        Ok(estimate)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(SocialSecurityEstimateResponse::from(estimate)))
}

/// List all imported/entered Social Security estimates for the caller.
#[utoipa::path(
    get,
    path = "/api/social-security-estimates",
    tag = "social_security",
    responses(
        (status = 200, description = "Estimates, newest first", body = [SocialSecurityEstimateResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/social-security-estimates")]
pub async fn list_estimates(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<SocialSecurityEstimate>> {
        let mut conn = pool.get()?;
        let rows = social_security_estimates::table
            .filter(social_security_estimates::user_id.eq(&user_id))
            .order(social_security_estimates::statement_date.desc())
            .select(SocialSecurityEstimate::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<SocialSecurityEstimateResponse> =
        rows.into_iter().map(SocialSecurityEstimateResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// Delete an estimate.
#[utoipa::path(
    delete,
    path = "/api/social-security-estimates/{id}",
    tag = "social_security",
    params(("id" = String, Path, description = "Estimate id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/social-security-estimates/{id}")]
pub async fn delete_estimate(
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
            social_security_estimates::table
                .filter(social_security_estimates::id.eq(&id))
                .filter(social_security_estimates::user_id.eq(&user_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Estimate not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}
