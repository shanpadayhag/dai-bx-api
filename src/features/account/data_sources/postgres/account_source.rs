use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::entities::account_profile::{
    AccentColor, AccountProfile, ProfileChanges, ThemeMode,
};
use crate::features::account::entities::account_security::AccountSecurity;
use crate::features::account::entities::time_tracking_settings::{
    TimeTrackingChanges, TimeTrackingSettings, TimeTrackingWrite,
};

#[derive(Default)]
pub struct PostgresAccountSource;

impl PostgresAccountSource {
    pub async fn find_profile(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<AccountProfile>, sqlx::Error> {
        sqlx::query_as!(
            AccountProfile,
            r#"SELECT name,
                      email::text AS "email!",
                      accent_color AS "accent_color: AccentColor",
                      theme AS "theme: ThemeMode"
               FROM users
               WHERE id = $1"#,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_time_tracking(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<TimeTrackingSettings>, sqlx::Error> {
        sqlx::query_as!(
            TimeTrackingSettings,
            r#"SELECT check_ins_enabled,
                      check_in_interval_minutes,
                      first_check_in_minutes,
                      to_char(working_hours_start, 'HH24:MI') AS "working_hours_start!",
                      to_char(working_hours_end, 'HH24:MI') AS "working_hours_end!",
                      tracked_category_id
               FROM users
               WHERE id = $1"#,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn update_time_tracking(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        time_tracking_changes: &TimeTrackingChanges,
    ) -> Result<TimeTrackingWrite, sqlx::Error> {
        let update_result = sqlx::query_as!(
            TimeTrackingSettings,
            r#"UPDATE users
               SET check_ins_enabled = coalesce($2, check_ins_enabled),
                   check_in_interval_minutes = coalesce($3, check_in_interval_minutes),
                   first_check_in_minutes = coalesce($4, first_check_in_minutes),
                   working_hours_start = coalesce($5::text::time, working_hours_start),
                   working_hours_end = coalesce($6::text::time, working_hours_end),
                   tracked_category_id = CASE WHEN $7 THEN $8 ELSE tracked_category_id END,
                   updated_at = now()
               WHERE id = $1
               RETURNING check_ins_enabled,
                         check_in_interval_minutes,
                         first_check_in_minutes,
                         to_char(working_hours_start, 'HH24:MI') AS "working_hours_start!",
                         to_char(working_hours_end, 'HH24:MI') AS "working_hours_end!",
                         tracked_category_id"#,
            user_id,
            time_tracking_changes.check_ins_enabled,
            time_tracking_changes.check_in_interval_minutes,
            time_tracking_changes.first_check_in_minutes,
            time_tracking_changes.working_hours_start.as_deref(),
            time_tracking_changes.working_hours_end.as_deref(),
            time_tracking_changes.tracked_category_change.is_some(),
            time_tracking_changes.tracked_category_change.flatten(),
        )
        .fetch_optional(executor)
        .await;
        match update_result {
            Ok(Some(time_tracking_settings)) => {
                Ok(TimeTrackingWrite::Saved(time_tracking_settings))
            }
            Ok(None) => Ok(TimeTrackingWrite::AccountMissing),
            Err(sqlx::Error::Database(database_error))
                if database_error.is_foreign_key_violation() =>
            {
                Ok(TimeTrackingWrite::CategoryMissing)
            }
            Err(update_error) => Err(update_error),
        }
    }

    pub async fn find_security(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        idle_limit_seconds: i64,
        absolute_limit_seconds: i64,
    ) -> Result<AccountSecurity, sqlx::Error> {
        sqlx::query_as!(
            AccountSecurity,
            r#"SELECT
                 (SELECT count(*)
                  FROM recovery_codes
                  WHERE user_id = $1 AND used_at IS NULL) AS "recovery_codes_remaining!",
                 (SELECT count(*)
                  FROM sessions
                  WHERE user_id = $1
                    AND revoked_at IS NULL
                    AND last_seen_at > now() - make_interval(secs => $2::float8)
                    AND created_at > now() - make_interval(secs => $3::float8)) AS "active_session_count!""#,
            user_id,
            idle_limit_seconds as f64,
            absolute_limit_seconds as f64,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn update_profile(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        profile_changes: &ProfileChanges,
    ) -> Result<Option<AccountProfile>, sqlx::Error> {
        sqlx::query_as!(
            AccountProfile,
            r#"UPDATE users
               SET name = coalesce($2, name),
                   accent_color = coalesce($3, accent_color),
                   theme = coalesce($4, theme),
                   updated_at = now()
               WHERE id = $1
               RETURNING name,
                         email::text AS "email!",
                         accent_color AS "accent_color: AccentColor",
                         theme AS "theme: ThemeMode""#,
            user_id,
            profile_changes.name.as_deref(),
            profile_changes.accent_color as Option<AccentColor>,
            profile_changes.theme as Option<ThemeMode>,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn change_email(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        new_email: &str,
    ) -> Result<bool, sqlx::Error> {
        let change_result = sqlx::query!(
            "UPDATE users
             SET email = $2::text::citext,
                 email_verified_at = coalesce(email_verified_at, now()),
                 updated_at = now()
             WHERE id = $1",
            user_id,
            new_email,
        )
        .execute(executor)
        .await;
        match change_result {
            Ok(change_outcome) => Ok(change_outcome.rows_affected() == 1),
            Err(sqlx::Error::Database(database_error)) if database_error.is_unique_violation() => {
                Ok(false)
            }
            Err(other_error) => Err(other_error),
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::categories::actions::create_category::CreateCategory;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_user_id(db_pool: &PgPool, email: &str) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: email.to_owned(),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    async fn seed_category_id(db_pool: &PgPool, user_id: Uuid) -> Uuid {
        CreateCategory::new()
            .execute(db_pool, user_id, "Focus", "#5b7fa6")
            .await
            .expect("the category should be created")
            .id
    }

    async fn update(
        db_pool: &PgPool,
        user_id: Uuid,
        time_tracking_changes: &TimeTrackingChanges,
    ) -> TimeTrackingWrite {
        PostgresAccountSource
            .update_time_tracking(db_pool, user_id, time_tracking_changes)
            .await
            .expect("the update should run")
    }

    #[sqlx::test]
    async fn updates_only_the_given_settings(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;

        let time_tracking_write = update(
            &db_pool,
            user_id,
            &TimeTrackingChanges {
                working_hours_end: Some(String::from("01:30")),
                tracked_category_change: Some(Some(category_id)),
                ..TimeTrackingChanges::default()
            },
        )
        .await;

        let expected_settings = TimeTrackingSettings {
            check_ins_enabled: true,
            check_in_interval_minutes: 30,
            first_check_in_minutes: 30,
            working_hours_start: String::from("16:00"),
            working_hours_end: String::from("01:30"),
            tracked_category_id: Some(category_id),
        };
        assert_eq!(
            time_tracking_write,
            TimeTrackingWrite::Saved(expected_settings.clone())
        );
        assert_eq!(
            PostgresAccountSource
                .find_time_tracking(&db_pool, user_id)
                .await
                .expect("the read should run"),
            Some(expected_settings)
        );
    }

    #[sqlx::test]
    async fn reports_another_person_s_category_and_a_missing_account(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;
        let category_change = TimeTrackingChanges {
            tracked_category_change: Some(Some(other_category_id)),
            ..TimeTrackingChanges::default()
        };

        assert_eq!(
            update(&db_pool, user_id, &category_change).await,
            TimeTrackingWrite::CategoryMissing
        );
        assert_eq!(
            update(&db_pool, Uuid::now_v7(), &TimeTrackingChanges::default()).await,
            TimeTrackingWrite::AccountMissing
        );
    }
}
