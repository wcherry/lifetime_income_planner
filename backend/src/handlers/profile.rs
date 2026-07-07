use actix_web::{get, put, web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::handlers::auth::format_validation;
use crate::models::{
    MaritalStatus, Profile, ProfileChangeset, ProfileResponse, UpsertProfileRequest,
};
use crate::schema::profiles;

/// Fetch the authenticated user's retirement profile.
#[utoipa::path(
    get,
    path = "/api/profile",
    tag = "profile",
    responses(
        (status = 200, description = "The user's profile", body = ProfileResponse),
        (status = 404, description = "No profile has been created yet"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/profile")]
pub async fn get_profile(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let pool = pool.clone();
    let profile = web::block(move || -> AppResult<Option<Profile>> {
        let mut conn = pool.get()?;
        let profile = profiles::table
            .filter(profiles::user_id.eq(&auth.user_id))
            .select(Profile::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(profile)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    match profile {
        Some(p) => Ok(HttpResponse::Ok().json(ProfileResponse::from(p))),
        None => Err(AppError::NotFound("No profile has been created yet".into())),
    }
}

/// Create or replace the authenticated user's retirement profile.
#[utoipa::path(
    put,
    path = "/api/profile",
    tag = "profile",
    request_body = UpsertProfileRequest,
    responses(
        (status = 200, description = "Profile saved", body = ProfileResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[put("/profile")]
pub async fn upsert_profile(
    pool: web::Data<DbPool>,
    auth: AuthUser,
    body: web::Json<UpsertProfileRequest>,
) -> AppResult<HttpResponse> {
    let payload = body.into_inner();
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(format_validation(&e)))?;

    validate_cross_field(&payload)?;

    let user_id = auth.user_id.clone();
    let changeset = ProfileChangeset {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.clone(),
        first_name: payload.first_name.trim().to_string(),
        last_name: payload.last_name.trim().to_string(),
        date_of_birth: payload.date_of_birth,
        marital_status: payload.marital_status.as_str().to_string(),
        filing_status: payload.filing_status.as_str().to_string(),
        state: payload.state.trim().to_uppercase(),
        retirement_date: payload.retirement_date,
        life_expectancy: payload.life_expectancy,
        // Only married profiles keep spouse data.
        spouse_first_name: married_field(&payload, payload.spouse_first_name.clone()),
        spouse_last_name: married_field(&payload, payload.spouse_last_name.clone()),
        spouse_date_of_birth: married_field(&payload, payload.spouse_date_of_birth),
        spouse_life_expectancy: married_field(&payload, payload.spouse_life_expectancy),
        updated_at: Utc::now().naive_utc(),
    };

    let pool = pool.clone();
    let profile = web::block(move || -> AppResult<Profile> {
        let mut conn = pool.get()?;

        let existing_id: Option<String> = profiles::table
            .filter(profiles::user_id.eq(&user_id))
            .select(profiles::id)
            .first(&mut conn)
            .optional()?;

        match existing_id {
            Some(id) => {
                // Preserve the original id; update everything else.
                diesel::update(profiles::table.filter(profiles::id.eq(&id)))
                    .set((
                        profiles::first_name.eq(&changeset.first_name),
                        profiles::last_name.eq(&changeset.last_name),
                        profiles::date_of_birth.eq(changeset.date_of_birth),
                        profiles::marital_status.eq(&changeset.marital_status),
                        profiles::filing_status.eq(&changeset.filing_status),
                        profiles::state.eq(&changeset.state),
                        profiles::retirement_date.eq(changeset.retirement_date),
                        profiles::life_expectancy.eq(changeset.life_expectancy),
                        profiles::spouse_first_name.eq(&changeset.spouse_first_name),
                        profiles::spouse_last_name.eq(&changeset.spouse_last_name),
                        profiles::spouse_date_of_birth.eq(changeset.spouse_date_of_birth),
                        profiles::spouse_life_expectancy.eq(changeset.spouse_life_expectancy),
                        profiles::updated_at.eq(changeset.updated_at),
                    ))
                    .execute(&mut conn)?;
            }
            None => {
                diesel::insert_into(profiles::table)
                    .values(&changeset)
                    .execute(&mut conn)?;
            }
        }

        let profile = profiles::table
            .filter(profiles::user_id.eq(&changeset.user_id))
            .select(Profile::as_select())
            .first(&mut conn)?;
        Ok(profile)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(ProfileResponse::from(profile)))
}

/// Keep spouse fields only for married profiles.
fn married_field<T>(payload: &UpsertProfileRequest, value: Option<T>) -> Option<T> {
    if payload.marital_status == MaritalStatus::Married {
        value
    } else {
        None
    }
}

/// Enforce relationships the per-field validators can't express.
fn validate_cross_field(p: &UpsertProfileRequest) -> AppResult<()> {
    if p.marital_status == MaritalStatus::Married {
        let missing = p.spouse_first_name.as_deref().unwrap_or("").trim().is_empty()
            || p.spouse_last_name.as_deref().unwrap_or("").trim().is_empty()
            || p.spouse_date_of_birth.is_none();
        if missing {
            return Err(AppError::BadRequest(
                "Spouse first name, last name, and date of birth are required for married profiles"
                    .into(),
            ));
        }
    }

    if p.retirement_date <= p.date_of_birth {
        return Err(AppError::BadRequest(
            "Retirement date must be after the date of birth".into(),
        ));
    }

    Ok(())
}
