use sqlx::PgExecutor;

use crate::features::invites::data_sources::postgres::invite_source::PostgresInviteSource;
use crate::features::invites::entities::invite_code::{hash_invite_code, normalize_invite_code};
use crate::features::invites::error::InviteError;

#[derive(Default)]
pub struct CheckInviteUsable {
    postgres_invite_source: PostgresInviteSource,
}

impl CheckInviteUsable {
    pub fn new() -> Self {
        Self {
            postgres_invite_source: PostgresInviteSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        entered_code: &str,
    ) -> Result<(), InviteError> {
        let normalized_code =
            normalize_invite_code(entered_code).ok_or(InviteError::InvalidCode)?;
        if self
            .postgres_invite_source
            .is_usable(executor, &hash_invite_code(&normalized_code))
            .await?
        {
            Ok(())
        } else {
            Err(InviteError::InvalidCode)
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::invites::actions::create_invite::CreateInvite;
    use crate::features::invites::actions::redeem_invite::RedeemInvite;
    use crate::features::invites::data_sources::postgres::invite_source::tests::seed_user;

    #[sqlx::test]
    async fn checks_a_code_without_using_it_up(db_pool: PgPool) {
        let created_invite = CreateInvite::new(&db_pool)
            .execute()
            .await
            .expect("creating should succeed");

        CheckInviteUsable::new()
            .execute(&db_pool, &created_invite.invite_code)
            .await
            .expect("a fresh code should be usable");
        CheckInviteUsable::new()
            .execute(&db_pool, &created_invite.invite_code)
            .await
            .expect("checking twice should not use it up");
        RedeemInvite::new()
            .execute(
                &db_pool,
                &created_invite.invite_code,
                seed_user(&db_pool).await,
            )
            .await
            .expect("redeeming should succeed");

        assert!(matches!(
            CheckInviteUsable::new()
                .execute(&db_pool, &created_invite.invite_code)
                .await,
            Err(InviteError::InvalidCode)
        ));
        assert!(matches!(
            CheckInviteUsable::new().execute(&db_pool, "nonsense").await,
            Err(InviteError::InvalidCode)
        ));
    }
}
