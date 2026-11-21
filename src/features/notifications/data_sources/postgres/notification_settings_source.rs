use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::notifications::entities::notification_settings::{
    NotificationChanges, NotificationSettings,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct PostgresNotificationSettingsSource;

impl PostgresNotificationSettingsSource {
    pub async fn find(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<NotificationSettings>, sqlx::Error> {
        sqlx::query_as!(
            NotificationSettings,
            r#"SELECT notify_check_ins AS check_ins,
                      notify_schedule_runs AS schedule_runs,
                      notify_blocks_starting AS blocks_starting,
                      notify_quiet_outside_working_hours AS quiet_outside_working_hours
               FROM users WHERE id = $1"#,
            user_id
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        notification_changes: &NotificationChanges,
    ) -> Result<Option<NotificationSettings>, sqlx::Error> {
        sqlx::query_as!(
            NotificationSettings,
            r#"UPDATE users
               SET notify_check_ins = COALESCE($2, notify_check_ins),
                   notify_schedule_runs = COALESCE($3, notify_schedule_runs),
                   notify_blocks_starting = COALESCE($4, notify_blocks_starting),
                   notify_quiet_outside_working_hours = COALESCE($5, notify_quiet_outside_working_hours)
               WHERE id = $1
               RETURNING notify_check_ins AS check_ins,
                         notify_schedule_runs AS schedule_runs,
                         notify_blocks_starting AS blocks_starting,
                         notify_quiet_outside_working_hours AS quiet_outside_working_hours"#,
            user_id,
            notification_changes.check_ins,
            notification_changes.schedule_runs,
            notification_changes.blocks_starting,
            notification_changes.quiet_outside_working_hours
        )
        .fetch_optional(executor)
        .await
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::notifications::data_sources::postgres::push_device_source::test_support::seed_user_with_session;

    #[sqlx::test]
    async fn changes_only_the_given_switches(db_pool: PgPool) {
        let (user_id, _) = seed_user_with_session(&db_pool, "a@example.com").await;
        let source = PostgresNotificationSettingsSource;

        let updated_settings = source
            .update(
                &db_pool,
                user_id,
                &NotificationChanges {
                    schedule_runs: Some(false),
                    ..NotificationChanges::default()
                },
            )
            .await
            .expect("update should succeed");

        assert_eq!(
            updated_settings,
            Some(NotificationSettings {
                check_ins: true,
                schedule_runs: false,
                blocks_starting: true,
                quiet_outside_working_hours: true,
            })
        );
        assert_eq!(source.find(&db_pool, Uuid::now_v7()).await.ok(), Some(None));
    }
}
