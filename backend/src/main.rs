mod aca;
mod auth;
mod config;
mod db;
mod error;
mod handlers;
mod insights;
mod irmaa;
mod models;
mod monte_carlo;
mod openapi;
mod plaid_client;
mod projection;
mod reconciliation;
mod schema;
mod tax;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::config::Config;
use crate::openapi::ApiDoc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let config = Config::from_env();
    let pool = db::build_pool(&config.database_url);
    db::run_migrations(&pool);
    db::seed_reference_data(&pool);

    let bind_host = config.host.clone();
    let bind_port = config.port;
    log::info!("Starting Lifetime Income Planner API on {bind_host}:{bind_port}");

    let config_data = web::Data::new(config);
    let pool_data = web::Data::new(pool);

    HttpServer::new(move || {
        // Permissive CORS is fine for local development; tighten for production.
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_header()
            .allow_any_method()
            .max_age(3600);

        App::new()
            .app_data(config_data.clone())
            .app_data(pool_data.clone())
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .service(
                web::scope("/api")
                    .service(handlers::health::health)
                    .service(handlers::auth::register)
                    .service(handlers::auth::login)
                    .service(handlers::auth::me)
                    .service(handlers::profile::get_profile)
                    .service(handlers::profile::upsert_profile)
                    .service(handlers::account::list_accounts)
                    .service(handlers::account::create_account)
                    .service(handlers::account::get_account)
                    .service(handlers::account::update_account)
                    .service(handlers::account::delete_account)
                    .service(handlers::spending::list_spending)
                    .service(handlers::spending::create_spending)
                    .service(handlers::spending::update_spending)
                    .service(handlers::spending::delete_spending)
                    .service(handlers::income::list_income)
                    .service(handlers::income::create_income)
                    .service(handlers::income::update_income)
                    .service(handlers::income::delete_income)
                    .service(handlers::life_event::list_life_events)
                    .service(handlers::life_event::create_life_event)
                    .service(handlers::life_event::update_life_event)
                    .service(handlers::life_event::delete_life_event)
                    .service(handlers::assumptions::get_assumptions)
                    .service(handlers::assumptions::upsert_assumptions)
                    .service(handlers::projection::get_projection)
                    .service(handlers::projection::what_if_projection)
                    .service(handlers::projection::optimize_projection)
                    .service(handlers::monte_carlo::run_monte_carlo_endpoint)
                    .service(handlers::quarterly_review::list_quarterly_reviews)
                    .service(handlers::quarterly_review::complete_quarterly_review)
                    .service(handlers::reports::get_tax_summary_csv)
                    .service(handlers::plaid::sandbox_connect)
                    .service(handlers::plaid::list_items)
                    .service(handlers::plaid::sync_item)
                    .service(handlers::plaid::delete_item)
                    .service(handlers::tax_document::import_tax_document)
                    .service(handlers::tax_document::list_tax_documents)
                    .service(handlers::tax_document::tax_document_year_summary)
                    .service(handlers::tax_document::delete_tax_document)
                    .service(handlers::spending_tracker::list_categories)
                    .service(handlers::spending_tracker::create_category)
                    .service(handlers::spending_tracker::update_category)
                    .service(handlers::spending_tracker::delete_category)
                    .service(handlers::spending_tracker::import_transactions)
                    .service(handlers::spending_tracker::create_manual_transaction)
                    .service(handlers::spending_tracker::list_months)
                    .service(handlers::spending_tracker::list_transactions)
                    .service(handlers::spending_tracker::set_transaction_category)
                    .service(handlers::spending_tracker::bulk_categorize_transactions)
                    .service(handlers::spending_tracker::quarter_summary)
                    .service(handlers::spending_tracker::year_summary)
                    .service(handlers::social_security_estimate::import_estimate)
                    .service(handlers::social_security_estimate::list_estimates)
                    .service(handlers::social_security_estimate::delete_estimate)
                    .service(handlers::insights::list_insights)
                    .service(handlers::collaborator::invite_collaborator)
                    .service(handlers::collaborator::list_collaborators)
                    .service(handlers::collaborator::list_invitations)
                    .service(handlers::collaborator::accept_invitation)
                    .service(handlers::collaborator::decline_invitation)
                    .service(handlers::collaborator::revoke_collaborator)
                    .service(handlers::collaborator::list_contexts)
                    .service(handlers::plan::list_plans)
                    .service(handlers::plan::compare_plans)
                    .service(handlers::plan::save_plan)
                    .service(handlers::plan::clone_plan)
                    .service(handlers::plan::update_plan_snapshot)
                    .service(handlers::plan::list_plan_versions)
                    .service(handlers::plan::restore_plan_version)
                    .service(handlers::plan::rename_plan)
                    .service(handlers::plan::load_plan)
                    .service(handlers::plan::delete_plan),
            )
            .service(
                SwaggerUi::new("/docs/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind((bind_host, bind_port))?
    .run()
    .await
}
