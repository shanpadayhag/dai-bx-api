use std::sync::Arc;
use std::time::Duration;

use daibx_api::app;
use daibx_api::core::config::app_config::AppConfig;
use daibx_api::core::database::database_connect_options;
use daibx_api::core::http_client::build_http_client;
use daibx_api::core::logging::init_logging;
use daibx_api::core::mail_transport::build_mail_transport;
use daibx_api::core::state::AppState;
use sqlx::postgres::PgPoolOptions;

const DATABASE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    init_logging();

    let app_config = AppConfig::from_environment().expect("configuration must be complete");
    let connect_options = database_connect_options(&app_config.database_url)
        .expect("DATABASE_URL must be a valid address");
    let db_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(DATABASE_ACQUIRE_TIMEOUT)
        .connect_with(connect_options)
        .await
        .expect("the database must be reachable at startup");

    let mail_transport = build_mail_transport(&app_config.smtp_settings);
    let app_state = AppState {
        db_pool,
        app_config: Arc::new(app_config),
        http_client: build_http_client(),
        mail_transport,
    };
    lambda_http::run(app(app_state)).await
}
