use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::security::recovery_code::{normalize_recovery_code, recovery_code_matches};
use crate::features::recovery_codes::data_sources::postgres::recovery_code_source::PostgresRecoveryCodeSource;
use crate::features::recovery_codes::entities::stored_recovery_code::StoredRecoveryCode;
use crate::features::recovery_codes::error::RecoveryCodeError;

#[derive(Default)]
pub struct UseRecoveryCode {
    postgres_recovery_code_source: PostgresRecoveryCodeSource,
}

impl UseRecoveryCode {
    pub fn new() -> Self {
        Self {
            postgres_recovery_code_source: PostgresRecoveryCodeSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        entered_code: &str,
    ) -> Result<Option<i64>, RecoveryCodeError> {
        let Some(normalized_code) = normalize_recovery_code(entered_code) else {
            return Ok(None);
        };
        let unused_codes = self
            .postgres_recovery_code_source
            .list_unused_for_update(&mut *connection, user_id)
            .await?;
        let matching_code_ids: Vec<Uuid> = unused_codes
            .iter()
            .filter(|stored_code| code_matches(&normalized_code, stored_code))
            .map(|stored_code| stored_code.id)
            .collect();
        let Some(&matching_code_id) = matching_code_ids.first() else {
            return Ok(None);
        };
        self.postgres_recovery_code_source
            .mark_used(&mut *connection, matching_code_id)
            .await?;
        let remaining_count = i64::try_from(unused_codes.len() - 1).unwrap_or(i64::MAX);
        Ok(Some(remaining_count))
    }
}

fn code_matches(normalized_code: &str, stored_code: &StoredRecoveryCode) -> bool {
    recovery_code_matches(normalized_code, &stored_code.salt, &stored_code.code_hash)
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::recovery_codes::actions::issue_recovery_codes::IssueRecoveryCodes;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_user_with_codes(db_pool: &PgPool) -> (Uuid, Vec<String>) {
        let user_id = CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: String::from("person@example.com"),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id;
        let recovery_codes = IssueRecoveryCodes::new()
            .execute(db_pool, user_id)
            .await
            .expect("issuing should succeed")
            .iter()
            .map(|recovery_code| recovery_code.to_string())
            .collect();
        (user_id, recovery_codes)
    }

    #[sqlx::test]
    async fn uses_a_code_once_and_counts_the_rest(db_pool: PgPool) {
        let (user_id, recovery_codes) = seed_user_with_codes(&db_pool).await;
        let mut connection = db_pool.acquire().await.expect("a connection");

        let first_use = UseRecoveryCode::new()
            .execute(&mut connection, user_id, &recovery_codes[3])
            .await
            .expect("using should run");
        let second_use = UseRecoveryCode::new()
            .execute(&mut connection, user_id, &recovery_codes[3])
            .await
            .expect("using should run");

        assert_eq!(first_use, Some(9));
        assert_eq!(second_use, None);
    }

    #[sqlx::test]
    async fn refuses_a_malformed_or_unknown_code(db_pool: PgPool) {
        let (user_id, _) = seed_user_with_codes(&db_pool).await;
        let mut connection = db_pool.acquire().await.expect("a connection");

        for entered_code in ["not a code", "0000-0000-0000"] {
            assert_eq!(
                UseRecoveryCode::new()
                    .execute(&mut connection, user_id, entered_code)
                    .await
                    .expect("using should run"),
                None
            );
        }
    }
}
