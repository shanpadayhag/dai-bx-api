use std::sync::Arc;

use sqlx::PgPool;

use crate::core::config::app_config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub app_config: Arc<AppConfig>,
}
