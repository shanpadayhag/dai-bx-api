use sqlx::PgExecutor;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::password_hash::hash_password;
use crate::core::security::recovery_code::{
    RECOVERY_CODE_COUNT, generate_recovery_code, normalize_recovery_code,
};
use crate::features::recovery_codes::data_sources::postgres::recovery_code_source::PostgresRecoveryCodeSource;
use crate::features::recovery_codes::error::RecoveryCodeError;

#[derive(Default)]
pub struct IssueRecoveryCodes {
    postgres_recovery_code_source: PostgresRecoveryCodeSource,
}

impl IssueRecoveryCodes {
    pub fn new() -> Self {
        Self {
            postgres_recovery_code_source: PostgresRecoveryCodeSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<Zeroizing<String>>, RecoveryCodeError> {
        let recovery_codes: Vec<Zeroizing<String>> = (0..RECOVERY_CODE_COUNT)
            .map(|_| generate_recovery_code())
            .collect();
        let mut code_phcs = Vec::with_capacity(RECOVERY_CODE_COUNT);
        for recovery_code in &recovery_codes {
            let normalized_code = normalize_recovery_code(recovery_code)
                .expect("a generated recovery code always normalizes");
            code_phcs.push(hash_password(normalized_code).await?);
        }
        self.postgres_recovery_code_source
            .replace_all(executor, user_id, &code_phcs)
            .await?;
        Ok(recovery_codes)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::recovery_codes::actions::count_unused_recovery_codes::CountUnusedRecoveryCodes;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_user_id(db_pool: &PgPool) -> Uuid {
        CreateUser::new()
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
            .id
    }

    #[sqlx::test]
    async fn issues_ten_distinct_grouped_codes_and_stores_ten_hashes(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;

        let recovery_codes = IssueRecoveryCodes::new()
            .execute(&db_pool, user_id)
            .await
            .expect("issuing should succeed");

        assert_eq!(recovery_codes.len(), 10);
        let mut distinct_codes: Vec<&str> =
            recovery_codes.iter().map(|code| code.as_str()).collect();
        distinct_codes.sort();
        distinct_codes.dedup();
        assert_eq!(distinct_codes.len(), 10);
        assert!(
            recovery_codes
                .iter()
                .all(|recovery_code| recovery_code.len() == 14)
        );
        assert_eq!(
            CountUnusedRecoveryCodes::new()
                .execute(&db_pool, user_id)
                .await
                .expect("counting should succeed"),
            10
        );
    }

    #[sqlx::test]
    async fn issuing_again_replaces_every_earlier_code(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        IssueRecoveryCodes::new()
            .execute(&db_pool, user_id)
            .await
            .expect("issuing should succeed");

        IssueRecoveryCodes::new()
            .execute(&db_pool, user_id)
            .await
            .expect("issuing again should succeed");

        assert_eq!(
            CountUnusedRecoveryCodes::new()
                .execute(&db_pool, user_id)
                .await
                .expect("counting should succeed"),
            10
        );
    }
}
