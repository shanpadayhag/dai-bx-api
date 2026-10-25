use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::invites::data_sources::postgres::invite_source::PostgresInviteSource;
use crate::features::invites::entities::invite_code::{hash_invite_code, normalize_invite_code};
use crate::features::invites::error::InviteError;

pub struct RedeemInvite {
    postgres_invite_source: PostgresInviteSource,
}

impl Default for RedeemInvite {
    fn default() -> Self {
        Self::new()
    }
}

impl RedeemInvite {
    pub fn new() -> Self {
        Self {
            postgres_invite_source: PostgresInviteSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        entered_code: &str,
        redeeming_user_id: Uuid,
    ) -> Result<(), InviteError> {
        let normalized_code =
            normalize_invite_code(entered_code).ok_or(InviteError::InvalidCode)?;
        self.postgres_invite_source
            .redeem(
                executor,
                &hash_invite_code(&normalized_code),
                redeeming_user_id,
            )
            .await?
            .map(|_| ())
            .ok_or(InviteError::InvalidCode)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::invites::actions::create_invite::CreateInvite;
    use crate::features::invites::data_sources::postgres::invite_source::tests::seed_user;
    use crate::features::invites::entities::invite_code::generate_invite_code;

    async fn redeem(db_pool: &PgPool, entered_code: &str) -> Result<(), InviteError> {
        let user_id = seed_user(db_pool).await;
        RedeemInvite::new()
            .execute(db_pool, entered_code, user_id)
            .await
    }

    #[sqlx::test]
    async fn redeems_a_created_invite_once_however_it_is_typed(db_pool: PgPool) {
        let created_invite = CreateInvite::new(&db_pool)
            .execute()
            .await
            .expect("creating should succeed");
        let typed_code = created_invite.invite_code.to_lowercase().replace('-', " ");

        assert!(redeem(&db_pool, &typed_code).await.is_ok());
        assert!(matches!(
            redeem(&db_pool, &created_invite.invite_code).await,
            Err(InviteError::InvalidCode)
        ));
    }

    #[sqlx::test]
    async fn refuses_unknown_malformed_and_expired_codes_the_same_way(db_pool: PgPool) {
        let expired_code = generate_invite_code();
        PostgresInviteSource
            .insert(
                &db_pool,
                &hash_invite_code(&normalize_invite_code(&expired_code).expect("normalizes")),
                -60,
            )
            .await
            .expect("insert should succeed");

        for refused_code in [
            generate_invite_code().to_string(),
            String::from("not a code"),
            String::new(),
            expired_code.to_string(),
        ] {
            assert!(
                matches!(
                    redeem(&db_pool, &refused_code).await,
                    Err(InviteError::InvalidCode)
                ),
                "{refused_code:?} should be refused like every other bad code"
            );
        }
    }

    #[sqlx::test]
    async fn lets_only_one_of_two_racing_sign_ups_use_a_code(db_pool: PgPool) {
        let created_invite = CreateInvite::new(&db_pool)
            .execute()
            .await
            .expect("creating should succeed");
        let first_user_id = seed_user(&db_pool).await;
        let second_user_id = seed_user(&db_pool).await;

        let redeem_in_own_transaction = |user_id: Uuid| {
            let db_pool = db_pool.clone();
            let invite_code = created_invite.invite_code.to_string();
            async move {
                let mut sign_up_transaction =
                    db_pool.begin().await.expect("transaction should begin");
                let redemption = RedeemInvite::new()
                    .execute(&mut *sign_up_transaction, &invite_code, user_id)
                    .await;
                sign_up_transaction
                    .commit()
                    .await
                    .expect("commit should succeed");
                redemption
            }
        };
        let (first_redemption, second_redemption) = tokio::join!(
            redeem_in_own_transaction(first_user_id),
            redeem_in_own_transaction(second_user_id)
        );

        assert_eq!(
            [&first_redemption, &second_redemption]
                .iter()
                .filter(|redemption| redemption.is_ok())
                .count(),
            1
        );
    }
}
