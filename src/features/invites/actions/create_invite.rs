use sqlx::PgPool;

use crate::features::invites::data_sources::postgres::invite_source::PostgresInviteSource;
use crate::features::invites::entities::invite_code::{
    CreatedInvite, INVITE_VALIDITY_SECONDS, generate_invite_code, hash_invite_code,
    normalize_invite_code,
};
use crate::features::invites::error::InviteError;

pub struct CreateInvite<'a> {
    db_pool: &'a PgPool,
    postgres_invite_source: PostgresInviteSource,
}

impl<'a> CreateInvite<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_invite_source: PostgresInviteSource,
        }
    }

    pub async fn execute(&self) -> Result<CreatedInvite, InviteError> {
        let invite_code = generate_invite_code();
        let normalized_code =
            normalize_invite_code(&invite_code).expect("a generated invite code always normalizes");
        let expires_at_utc = self
            .postgres_invite_source
            .insert(
                self.db_pool,
                &hash_invite_code(&normalized_code),
                INVITE_VALIDITY_SECONDS,
            )
            .await?;
        Ok(CreatedInvite {
            invite_code,
            expires_at_utc,
        })
    }
}
