use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::{ConfirmIdentity, IdentityProof};
use crate::features::account::error::AccountError;
use crate::features::auth_challenges::actions::create_auth_challenge::CreateAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::sessions::entities::current_session::CurrentSession;

pub struct StartAuthenticatorReplacement<'a> {
    db_pool: &'a PgPool,
    confirm_identity: ConfirmIdentity<'a>,
    create_auth_challenge: CreateAuthChallenge<'a>,
}

impl<'a> StartAuthenticatorReplacement<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            confirm_identity: ConfirmIdentity::new(app_state),
            create_auth_challenge: CreateAuthChallenge::new(&app_state.app_config.totp_keyring),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        identity_proof: IdentityProof<'_>,
        request_context: &RequestContext,
    ) -> Result<Zeroizing<String>, AccountError> {
        let mut start_transaction = self.db_pool.begin().await?;
        let signed_in_user = self
            .confirm_identity
            .execute(
                &mut start_transaction,
                current_session,
                identity_proof,
                request_context,
            )
            .await?;
        let challenge_token = self
            .create_auth_challenge
            .execute(
                &mut *start_transaction,
                signed_in_user.id,
                AuthChallengePurpose::MfaReplace,
            )
            .await?;
        start_transaction.commit().await?;
        Ok(challenge_token)
    }
}
