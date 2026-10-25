use sqlx::PgConnection;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::password_hash::verify_password;
use crate::core::security::recovery_code::{legacy_recovery_code_matches, normalize_recovery_code};
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
        let mut matching_code_id = None;
        for stored_code in &unused_codes {
            if code_matches(&normalized_code, stored_code).await? {
                matching_code_id = Some(stored_code.id);
                break;
            }
        }
        let Some(matching_code_id) = matching_code_id else {
            return Ok(None);
        };
        self.postgres_recovery_code_source
            .mark_used(&mut *connection, matching_code_id)
            .await?;
        let remaining_count = i64::try_from(unused_codes.len() - 1).unwrap_or(i64::MAX);
        Ok(Some(remaining_count))
    }
}

async fn code_matches(
    normalized_code: &Zeroizing<String>,
    stored_code: &StoredRecoveryCode,
) -> Result<bool, RecoveryCodeError> {
    if let Some(code_phc) = &stored_code.code_phc {
        let code_verification = verify_password(normalized_code.clone(), code_phc.clone()).await?;
        return Ok(code_verification.is_valid);
    }
    Ok(
        match (&stored_code.legacy_salt, &stored_code.legacy_code_hash) {
            (Some(legacy_salt), Some(legacy_code_hash)) => {
                legacy_recovery_code_matches(normalized_code, legacy_salt, legacy_code_hash)
            }
            _ => false,
        },
    )
}

#[cfg(test)]
mod tests {
    use sha2::Digest;
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
    async fn stores_new_codes_as_argon2id_hashes_only(db_pool: PgPool) {
        let (user_id, _) = seed_user_with_codes(&db_pool).await;
        let mut connection = db_pool.acquire().await.expect("a connection");

        let stored_codes = PostgresRecoveryCodeSource
            .list_unused_for_update(&mut *connection, user_id)
            .await
            .expect("listing should succeed");

        assert_eq!(stored_codes.len(), 10);
        assert!(stored_codes.iter().all(|stored_code| {
            stored_code
                .code_phc
                .as_deref()
                .is_some_and(|code_phc| code_phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"))
                && stored_code.legacy_salt.is_none()
                && stored_code.legacy_code_hash.is_none()
        }));
    }

    #[sqlx::test]
    async fn still_accepts_a_code_saved_with_the_old_salted_hash(db_pool: PgPool) {
        let (user_id, _) = seed_user_with_codes(&db_pool).await;
        let legacy_salt = [7_u8; 16];
        let legacy_code_hash: [u8; 32] = sha2::Sha256::new()
            .chain_update(legacy_salt)
            .chain_update("K7QM2XRD9FTB".as_bytes())
            .finalize()
            .into();
        sqlx::query(
            "INSERT INTO recovery_codes (id, user_id, salt, code_hash) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(legacy_salt.to_vec())
        .bind(legacy_code_hash.to_vec())
        .execute(&db_pool)
        .await
        .expect("seeding a legacy code should succeed");
        let mut connection = db_pool.acquire().await.expect("a connection");

        let legacy_use = UseRecoveryCode::new()
            .execute(&mut connection, user_id, "k7qm-2xrd-9ftb")
            .await
            .expect("using should run");

        assert_eq!(legacy_use, Some(10));
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
