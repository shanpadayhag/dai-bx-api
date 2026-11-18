use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::{
    ConfirmIdentity, ConfirmedIdentity, IdentityProof,
};
use crate::features::account::error::AccountError;
use crate::features::auth_challenges::actions::create_auth_challenge::CreateAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::sessions::entities::current_session::CurrentSession;

pub struct StartAuthenticatorReplacement<'a> {
    confirm_identity: ConfirmIdentity<'a>,
    create_auth_challenge: CreateAuthChallenge<'a>,
}

impl<'a> StartAuthenticatorReplacement<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
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
        let ConfirmedIdentity {
            signed_in_user,
            transaction: mut start_transaction,
        } = self
            .confirm_identity
            .execute(current_session, identity_proof, request_context)
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
