mod auth;
mod config;
mod db;
mod error;
mod handlers;
mod models;
mod openapi;
mod projection;
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
                    .service(handlers::reports::get_tax_summary_csv)
                    .service(handlers::plan::list_plans)
                    .service(handlers::plan::save_plan)
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
