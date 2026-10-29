//! Annual planning wizard (roadmap Phase 5, feature 8): a guided checklist
//! that sequences through the existing settings pages once a year rather
//! than duplicating their forms — each step links out to the real page and
//! shows a live status hint pulled from the user's actual data.

use actix_web::{get, post, web, HttpResponse};
use chrono::{Datelike, NaiveDateTime, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    AnnualPlanReview, AnnualWizardResponse, AnnualWizardStep, Assumptions, NewAnnualPlanReview,
    Profile,
};
use crate::schema::{
    accounts, annual_plan_reviews, assumptions, income_sources, life_events, profiles,
    spending_items,
};

fn fmt_date(d: NaiveDateTime) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn plural(count: i64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// This year's guided planning checklist, with live status hints from the
/// user's actual data (roadmap Phase 5, feature 8).
#[utoipa::path(
    get,
    path = "/api/annual-wizard",
    tag = "annual_wizard",
    responses(
        (status = 200, description = "This year's guided planning checklist", body = AnnualWizardResponse),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[get("/annual-wizard")]
pub async fn get_annual_wizard(pool: web::Data<DbPool>, auth: AuthUser) -> AppResult<HttpResponse> {
    let year = Utc::now().year();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();

    let (profile, assumptions_row, account_count, income_count, spending_count, life_event_count, completed) =
        web::block(
            move || -> AppResult<(Option<Profile>, Option<Assumptions>, i64, i64, i64, i64, bool)> {
                let mut conn = pool.get()?;
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
                let account_count: i64 = accounts::table
                    .filter(accounts::user_id.eq(&user_id))
                    .count()
                    .get_result(&mut conn)?;
                let income_count: i64 = income_sources::table
                    .filter(income_sources::user_id.eq(&user_id))
                    .count()
                    .get_result(&mut conn)?;
                let spending_count: i64 = spending_items::table
                    .filter(spending_items::user_id.eq(&user_id))
                    .count()
                    .get_result(&mut conn)?;
                let life_event_count: i64 = life_events::table
                    .filter(life_events::user_id.eq(&user_id))
                    .count()
                    .get_result(&mut conn)?;
                let completed = annual_plan_reviews::table
                    .filter(annual_plan_reviews::user_id.eq(&user_id))
                    .filter(annual_plan_reviews::year.eq(year))
                    .select(AnnualPlanReview::as_select())
                    .first::<AnnualPlanReview>(&mut conn)
                    .optional()?
                    .is_some();
                Ok((
                    profile,
                    assumptions_row,
                    account_count,
                    income_count,
                    spending_count,
                    life_event_count,
                    completed,
                ))
            },
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

    let steps = vec![
        AnnualWizardStep {
            key: "profile".into(),
            label: "Review your profile".into(),
            route: "/profile".into(),
            hint: profile
                .map(|p| format!("Last updated {}", fmt_date(p.updated_at)))
                .unwrap_or_else(|| "Not yet created".into()),
        },
        AnnualWizardStep {
            key: "accounts".into(),
            label: "Update account balances".into(),
            route: "/accounts".into(),
            hint: plural(account_count, "account"),
        },
        AnnualWizardStep {
            key: "income_spending".into(),
            label: "Review income and spending".into(),
            route: "/income".into(),
            hint: format!(
                "{}, {}",
                plural(income_count, "income source"),
                plural(spending_count, "spending item")
            ),
        },
        AnnualWizardStep {
            key: "life_events".into(),
            label: "Update life events for the year ahead".into(),
            route: "/life-events".into(),
            hint: plural(life_event_count, "life event"),
        },
        AnnualWizardStep {
            key: "assumptions".into(),
            label: "Review your planning assumptions".into(),
            route: "/assumptions".into(),
            hint: assumptions_row
                .map(|a| format!("Last updated {}", fmt_date(a.updated_at)))
                .unwrap_or_else(|| "Using defaults".into()),
        },
        AnnualWizardStep {
            key: "projection".into(),
            label: "Review your updated projection".into(),
            route: "/projection".into(),
            hint: "See how this year's changes affect your plan".into(),
        },
    ];

    Ok(HttpResponse::Ok().json(AnnualWizardResponse {
        year,
        completed,
        steps,
    }))
}

/// Mark this year's planning pass complete. Idempotent — completing an
/// already-completed year is a no-op.
#[utoipa::path(
    post,
    path = "/api/annual-wizard/complete",
    tag = "annual_wizard",
    responses(
        (status = 204, description = "This year's planning marked complete"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = []))
)]
#[post("/annual-wizard/complete")]
pub async fn complete_annual_wizard(
    pool: web::Data<DbPool>,
    auth: AuthUser,
) -> AppResult<HttpResponse> {
    let year = Utc::now().year();
    let user_id = auth.user_id.clone();
    let pool = pool.clone();
    web::block(move || -> AppResult<()> {
        let mut conn = pool.get()?;
        let existing = annual_plan_reviews::table
            .filter(annual_plan_reviews::user_id.eq(&user_id))
            .filter(annual_plan_reviews::year.eq(year))
            .select(AnnualPlanReview::as_select())
            .first::<AnnualPlanReview>(&mut conn)
            .optional()?;
        if existing.is_none() {
            let new_row = NewAnnualPlanReview {
                id: Uuid::new_v4().to_string(),
                user_id,
                year,
                completed_at: Utc::now().naive_utc(),
            };
            diesel::insert_into(annual_plan_reviews::table)
                .values(&new_row)
                .execute(&mut conn)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_uses_singular_noun_for_exactly_one() {
        assert_eq!(plural(1, "account"), "1 account");
        assert_eq!(plural(0, "account"), "0 accounts");
        assert_eq!(plural(3, "account"), "3 accounts");
    }
}
