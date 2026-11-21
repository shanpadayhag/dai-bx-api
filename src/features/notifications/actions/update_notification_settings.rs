use sqlx::PgPool;
use uuid::Uuid;

use crate::features::notifications::data_sources::postgres::notification_settings_source::PostgresNotificationSettingsSource;
use crate::features::notifications::entities::notification_settings::{
    NotificationChanges, NotificationSettings,
};
use crate::features::notifications::error::NotificationError;

pub struct UpdateNotificationSettings<'a> {
    db_pool: &'a PgPool,
    postgres_notification_settings_source: PostgresNotificationSettingsSource,
}

impl<'a> UpdateNotificationSettings<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_notification_settings_source: PostgresNotificationSettingsSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        notification_changes: &NotificationChanges,
    ) -> Result<NotificationSettings, NotificationError> {
        self.postgres_notification_settings_source
            .update(self.db_pool, user_id, notification_changes)
            .await?
            .ok_or(NotificationError::NotSignedIn)
    }
}
