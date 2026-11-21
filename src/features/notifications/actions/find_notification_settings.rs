use sqlx::PgPool;
use uuid::Uuid;

use crate::features::notifications::data_sources::postgres::notification_settings_source::PostgresNotificationSettingsSource;
use crate::features::notifications::entities::notification_settings::NotificationSettings;
use crate::features::notifications::error::NotificationError;

pub struct FindNotificationSettings<'a> {
    db_pool: &'a PgPool,
    postgres_notification_settings_source: PostgresNotificationSettingsSource,
}

impl<'a> FindNotificationSettings<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_notification_settings_source: PostgresNotificationSettingsSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<NotificationSettings, NotificationError> {
        self.postgres_notification_settings_source
            .find(self.db_pool, user_id)
            .await?
            .ok_or(NotificationError::NotSignedIn)
    }
}
