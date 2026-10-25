use axum::Json;
use axum::http::{HeaderValue, header};
use axum::response::{AppendHeaders, IntoResponse, Response};

use crate::core::cookie::build_host_cookie;
use crate::features::auth::actions::sign_in::SignInOutcome;
use crate::features::auth::responses::next_step_response::NextStepResponse;
use crate::features::auth_challenges::entities::auth_challenge::{
    AUTH_CHALLENGE_COOKIE_NAME, AUTH_CHALLENGE_VALID_FOR_SECONDS,
};

pub fn sign_in_outcome_response(
    sign_in_outcome: SignInOutcome,
    mut set_cookie_values: Vec<HeaderValue>,
) -> Response {
    let next_step = match sign_in_outcome {
        SignInOutcome::VerifyEmail => NextStepResponse::verify_email(),
        SignInOutcome::SetUpAuthenticator { challenge_token } => {
            set_cookie_values.push(challenge_cookie(&challenge_token));
            NextStepResponse::set_up_authenticator()
        }
        SignInOutcome::EnterCode { challenge_token } => {
            set_cookie_values.push(challenge_cookie(&challenge_token));
            NextStepResponse::enter_code()
        }
        SignInOutcome::SignedIn { session_tokens } => {
            set_cookie_values.extend(session_tokens.to_set_cookie_values());
            NextStepResponse::signed_in()
        }
    };
    (
        AppendHeaders(
            set_cookie_values
                .into_iter()
                .map(|set_cookie_value| (header::SET_COOKIE, set_cookie_value)),
        ),
        Json(next_step),
    )
        .into_response()
}

fn challenge_cookie(challenge_token: &str) -> HeaderValue {
    build_host_cookie(
        AUTH_CHALLENGE_COOKIE_NAME,
        challenge_token,
        AUTH_CHALLENGE_VALID_FOR_SECONDS,
    )
}
