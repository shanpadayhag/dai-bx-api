use std::sync::Arc;
use std::time::Duration;

use daibx_api::app;
use daibx_api::core::config::app_config::AppConfig;
use daibx_api::core::state::AppState;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

const DATABASE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .init();

    let app_config = AppConfig::from_environment().expect("configuration must be complete");
    let db_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(DATABASE_ACQUIRE_TIMEOUT)
        .connect(&app_config.database_url)
        .await
        .expect("the database must be reachable at startup");

    let app_state = AppState {
        db_pool,
        app_config: Arc::new(app_config),
    };
    lambda_http::run(app(app_state)).await
}
