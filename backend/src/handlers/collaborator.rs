//! Collaboration (roadmap Phase 6, feature 7): invite/accept/revoke access
//! grants, and list the contexts the caller can act as. Read-only
//! enforcement for advisors happens once, centrally, in `AuthUser`
//! (`crate::auth`) — these handlers only manage the grants themselves.

use actix_web::{delete, get, post, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::{AccessRole, AuthUser};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{
    Collaborator, CollaborationContext, CollaboratorResponse, InvitationResponse,
    InviteCollaboratorRequest, NewCollaborator, User,
};
use crate::schema::{collaborators, users};

/// Granting a third party access is more sensitive than editing data, so
/// managing collaborators is restricted to the data's actual owner — even a
/// spouse with full read-write access can't invite or revoke on the owner's
/// behalf while acting in that context.
fn require_owner(auth: &AuthUser) -> AppResult<()> {
    if auth.role == AccessRole::Owner {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Only the account owner can manage collaborators".into(),
        ))
    }
}

/// Invite another registered user to collaborate on the caller's own data.
/// There's no email delivery infra, so the invitee must already have an
/// account; the grant starts "pending" until they accept it.
#[utoipa::path(
    post,
    path = "/api/collaborators",
    tag = "collaboration",
    request_body = InviteCollaboratorRequest,
    responses(
        (status = 201, description = "Invitation created", body = CollaboratorResponse),
        (status = 400, description = "Validation error or no account for that email"),
        (status = 409, description = "Already invited or has access"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/collaborators")]
pub async fn invite_collaborator(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<InviteCollaboratorRequest>,
) -> AppResult<HttpResponse> {
    require_owner(&auth)?;
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    let email = payload.email.trim().to_lowercase();
    let owner_id = auth.user_id.clone();
    let role_str = payload.role.as_str().to_string();

    let pool = pool.clone();
    let collaborator = web::block(move || -> AppResult<Collaborator> {
        let mut conn = pool.get()?;
        let invitee = users::table
            .filter(users::email.eq(&email))
            .select(User::as_select())
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| {
                AppError::BadRequest(
                    "No account found for that email — they need to register first.".into(),
                )
            })?;

        if invitee.id == owner_id {
            return Err(AppError::BadRequest("You can't invite yourself".into()));
        }

        let existing: i64 = collaborators::table
            .filter(collaborators::owner_user_id.eq(&owner_id))
            .filter(collaborators::collaborator_user_id.eq(&invitee.id))
            .count()
            .get_result(&mut conn)?;
        if existing > 0 {
            return Err(AppError::Conflict(
                "This person is already invited or already has access".into(),
            ));
        }

        let id = Uuid::new_v4().to_string();
        let new_collaborator = NewCollaborator {
            id: id.clone(),
            owner_user_id: owner_id.clone(),
            collaborator_user_id: invitee.id.clone(),
            invited_email: invitee.email.clone(),
            role: role_str.clone(),
            status: "pending".to_string(),
            updated_at: Utc::now().naive_utc(),
        };
        diesel::insert_into(collaborators::table)
            .values(&new_collaborator)
            .execute(&mut conn)?;

        let row = collaborators::table
            .filter(collaborators::id.eq(&id))
            .select(Collaborator::as_select())
            .first(&mut conn)?;
        Ok(row)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Created().json(CollaboratorResponse::from(collaborator)))
}

/// List everyone with (or pending) access to the active context's data.
#[utoipa::path(
    get,
    path = "/api/collaborators",
    tag = "collaboration",
    responses(
        (status = 200, description = "Grants, newest first", body = [CollaboratorResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/collaborators")]
pub async fn list_collaborators(
    pool: web::Data<DbPool>,
    auth: AuthUser,
) -> AppResult<HttpResponse> {
    require_owner(&auth)?;
    let owner_id = auth.user_id.clone();
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<Collaborator>> {
        let mut conn = pool.get()?;
        let rows = collaborators::table
            .filter(collaborators::owner_user_id.eq(&owner_id))
            .order(collaborators::created_at.desc())
            .select(Collaborator::as_select())
            .load(&mut conn)?;
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<CollaboratorResponse> = rows.into_iter().map(CollaboratorResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

/// List pending invitations addressed to the caller's own identity
/// (regardless of any active collaboration context).
#[utoipa::path(
    get,
    path = "/api/collaborators/invitations",
    tag = "collaboration",
    responses(
        (status = 200, description = "Pending invitations", body = [InvitationResponse]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/collaborators/invitations")]
pub async fn list_invitations(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let caller_id = auth.caller_id.clone();
    let pool = pool.clone();
    let rows = web::block(move || -> AppResult<Vec<(Collaborator, String)>> {
        let mut conn = pool.get()?;
        let rows: Vec<Collaborator> = collaborators::table
            .filter(collaborators::collaborator_user_id.eq(&caller_id))
            .filter(collaborators::status.eq("pending"))
            .order(collaborators::created_at.desc())
            .select(Collaborator::as_select())
            .load(&mut conn)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let owner_email: String = users::table
                .filter(users::id.eq(&row.owner_user_id))
                .select(users::email)
                .first(&mut conn)?;
            out.push((row, owner_email));
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let body: Vec<InvitationResponse> = rows
        .into_iter()
        .map(|(row, owner_email)| InvitationResponse {
            id: row.id,
            owner_email,
            role: row.role,
            created_at: row.created_at,
        })
        .collect();
    Ok(HttpResponse::Ok().json(body))
}

async fn set_invitation_status(
    pool: &web::Data<DbPool>,
    caller_id: String,
    id: String,
    new_status: &'static str,
) -> AppResult<Collaborator> {
    let pool = pool.clone();
    let row = web::block(move || -> AppResult<Collaborator> {
        let mut conn = pool.get()?;
        let owned: i64 = collaborators::table
            .filter(collaborators::id.eq(&id))
            .filter(collaborators::collaborator_user_id.eq(&caller_id))
            .filter(collaborators::status.eq("pending"))
            .count()
            .get_result(&mut conn)?;
        if owned == 0 {
            return Err(AppError::NotFound("Invitation not found".into()));
        }
        diesel::update(collaborators::table.filter(collaborators::id.eq(&id)))
            .set((
                collaborators::status.eq(new_status),
                collaborators::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&mut conn)?;
        let row = collaborators::table
            .filter(collaborators::id.eq(&id))
            .select(Collaborator::as_select())
            .first(&mut conn)?;
        Ok(row)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(row)
}

/// Accept a pending invitation addressed to the caller.
#[utoipa::path(
    post,
    path = "/api/collaborators/{id}/accept",
    tag = "collaboration",
    params(("id" = String, Path, description = "Invitation id")),
    responses(
        (status = 200, description = "Accepted", body = CollaboratorResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/collaborators/{id}/accept")]
pub async fn accept_invitation(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let row = set_invitation_status(&pool, auth.caller_id.clone(), path.into_inner(), "active").await?;
    Ok(HttpResponse::Ok().json(CollaboratorResponse::from(row)))
}

/// Decline a pending invitation addressed to the caller.
#[utoipa::path(
    post,
    path = "/api/collaborators/{id}/decline",
    tag = "collaboration",
    params(("id" = String, Path, description = "Invitation id")),
    responses(
        (status = 200, description = "Declined", body = CollaboratorResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/collaborators/{id}/decline")]
pub async fn decline_invitation(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let row =
        set_invitation_status(&pool, auth.caller_id.clone(), path.into_inner(), "declined").await?;
    Ok(HttpResponse::Ok().json(CollaboratorResponse::from(row)))
}

/// Revoke a grant on the active context's data.
#[utoipa::path(
    delete,
    path = "/api/collaborators/{id}",
    tag = "collaboration",
    params(("id" = String, Path, description = "Collaborator grant id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "Not found"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[delete("/collaborators/{id}")]
pub async fn revoke_collaborator(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    require_owner(&auth)?;
    let id = path.into_inner();
    let owner_id = auth.user_id.clone();
    let pool = pool.clone();
    let deleted = web::block(move || -> AppResult<usize> {
        let mut conn = pool.get()?;
        let n = diesel::delete(
            collaborators::table
                .filter(collaborators::id.eq(&id))
                .filter(collaborators::owner_user_id.eq(&owner_id)),
        )
        .execute(&mut conn)?;
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if deleted == 0 {
        return Err(AppError::NotFound("Collaborator not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

/// List the contexts the caller can act as: their own data, plus any owner's
/// data they've been granted (and accepted) access to. Drives the
/// frontend's context switcher; the returned `user_id` is what to send back
/// as `X-Context-User`.
#[utoipa::path(
    get,
    path = "/api/collaborators/contexts",
    tag = "collaboration",
    responses(
        (status = 200, description = "Available contexts", body = [CollaborationContext]),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/collaborators/contexts")]
pub async fn list_contexts(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let caller_id = auth.caller_id.clone();
    let pool = pool.clone();
    let (me, grants) = web::block(move || -> AppResult<(User, Vec<(Collaborator, String)>)> {
        let mut conn = pool.get()?;
        let me = users::table
            .filter(users::id.eq(&caller_id))
            .select(User::as_select())
            .first(&mut conn)?;
        let rows: Vec<Collaborator> = collaborators::table
            .filter(collaborators::collaborator_user_id.eq(&caller_id))
            .filter(collaborators::status.eq("active"))
            .select(Collaborator::as_select())
            .load(&mut conn)?;
        let mut grants = Vec::with_capacity(rows.len());
        for row in rows {
            let owner_email: String = users::table
                .filter(users::id.eq(&row.owner_user_id))
                .select(users::email)
                .first(&mut conn)?;
            grants.push((row, owner_email));
        }
        Ok((me, grants))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let mut contexts = vec![CollaborationContext {
        user_id: me.id,
        label: format!("{} (you)", me.email),
        role: "owner".to_string(),
    }];
    for (row, owner_email) in grants {
        contexts.push(CollaborationContext {
            user_id: row.owner_user_id,
            label: owner_email,
            role: row.role,
        });
    }
    Ok(HttpResponse::Ok().json(contexts))
}
