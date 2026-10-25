use serde::Serialize;

use crate::features::auth::actions::show_authenticator_setup::AuthenticatorSetup;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorSetupResponse {
    pub otpauth_uri: String,
    pub manual_entry_key: String,
}

impl From<AuthenticatorSetup> for AuthenticatorSetupResponse {
    fn from(authenticator_setup: AuthenticatorSetup) -> Self {
        Self {
            otpauth_uri: authenticator_setup.otpauth_uri,
            manual_entry_key: authenticator_setup.manual_entry_key,
        }
    }
}
