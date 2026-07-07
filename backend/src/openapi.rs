use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::handlers;
use crate::models;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Lifetime Income Planner API",
        version = "0.1.0",
        description = "Retirement planning platform — Phase 1: accounts, authentication, and retirement profile setup."
    ),
    paths(
        handlers::health::health,
        handlers::auth::register,
        handlers::auth::login,
        handlers::auth::me,
        handlers::profile::get_profile,
        handlers::profile::upsert_profile,
        handlers::account::list_accounts,
        handlers::account::create_account,
        handlers::account::get_account,
        handlers::account::update_account,
        handlers::account::delete_account,
        handlers::spending::list_spending,
        handlers::spending::create_spending,
        handlers::spending::update_spending,
        handlers::spending::delete_spending,
        handlers::income::list_income,
        handlers::income::create_income,
        handlers::income::update_income,
        handlers::income::delete_income,
        handlers::life_event::list_life_events,
        handlers::life_event::create_life_event,
        handlers::life_event::update_life_event,
        handlers::life_event::delete_life_event,
        handlers::assumptions::get_assumptions,
        handlers::assumptions::upsert_assumptions,
        handlers::projection::get_projection,
        handlers::plan::list_plans,
        handlers::plan::save_plan,
        handlers::plan::rename_plan,
        handlers::plan::load_plan,
        handlers::plan::delete_plan,
    ),
    components(schemas(
        models::RegisterRequest,
        models::LoginRequest,
        models::AuthResponse,
        models::UserResponse,
        models::UpsertProfileRequest,
        models::ProfileResponse,
        models::MaritalStatus,
        models::FilingStatus,
        models::AccountRequest,
        models::AccountResponse,
        models::AccountCategory,
        models::AccountType,
        models::AccountOwner,
        models::SpendingRequest,
        models::SpendingResponse,
        models::SpendingCategory,
        models::SpendingFrequency,
        models::IncomeRequest,
        models::IncomeResponse,
        models::IncomeType,
        models::IncomeFrequency,
        models::Taxability,
        models::IncomeOwner,
        models::LifeEventRequest,
        models::LifeEventResponse,
        models::LifeEventType,
        models::CashFlowDirection,
        models::EventRecurrence,
        models::AssumptionsRequest,
        models::AssumptionsResponse,
        models::ProjectionResponse,
        models::ProjectionAssumptions,
        models::ProjectionSummary,
        models::YearProjection,
        models::LifeEventOccurrence,
        models::Milestone,
        models::QuarterProjection,
        models::QuarterWithdrawal,
        models::SavePlanRequest,
        models::PlanResponse,
        models::PlanContents,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "auth", description = "Account registration and authentication"),
        (name = "profile", description = "Retirement profile setup"),
        (name = "accounts", description = "Financial account management"),
        (name = "spending", description = "Spending assumptions"),
        (name = "income", description = "Income sources"),
        (name = "life_events", description = "Life events engine"),
        (name = "assumptions", description = "Inflation and ROI assumptions"),
        (name = "projection", description = "Projection engine and quarterly withdrawal schedule"),
        (name = "plans", description = "Save and load retirement plans"),
        (name = "health", description = "Service health"),
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
    }
}
