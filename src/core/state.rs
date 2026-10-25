use std::sync::Arc;

use reqwest::Client;
use sqlx::PgPool;

use crate::core::config::app_config::AppConfig;
use crate::core::mail_transport::MailTransport;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub app_config: Arc<AppConfig>,
    pub http_client: Client,
    pub mail_transport: MailTransport,
}
