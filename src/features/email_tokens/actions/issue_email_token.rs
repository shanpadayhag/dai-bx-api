use sqlx::PgConnection;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::opaque_token::{generate_opaque_token, hash_opaque_token};
use crate::features::email_tokens::data_sources::postgres::email_token_source::PostgresEmailTokenSource;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::email_tokens::error::EmailTokenError;

#[derive(Default)]
pub struct IssueEmailToken {
    postgres_email_token_source: PostgresEmailTokenSource,
}

impl IssueEmailToken {
    pub fn new() -> Self {
        Self {
            postgres_email_token_source: PostgresEmailTokenSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        purpose: EmailTokenPurpose,
        new_email: Option<&str>,
    ) -> Result<Zeroizing<String>, EmailTokenError> {
        self.postgres_email_token_source
            .retire_unused(&mut *connection, user_id, purpose)
            .await?;
        let link_token = generate_opaque_token();
        self.postgres_email_token_source
            .insert(
                &mut *connection,
                user_id,
                purpose,
                &hash_opaque_token(&link_token),
                new_email,
                purpose.valid_for_seconds(),
            )
            .await?;
        Ok(link_token)
    }
}
