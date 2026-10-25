use axum::Router;
use axum::routing::{get, post};

use crate::core::state::AppState;
use crate::features::auth::handlers::{
    get_mfa_setup, get_session, post_mfa_setup_confirm, post_mfa_verify, post_password_reset,
    post_password_reset_complete, post_refresh, post_resend_verification_email, post_sign_in,
    post_sign_out, post_sign_out_everywhere, post_sign_up, post_verify_email,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sign-up", post(post_sign_up::post_sign_up))
        .route("/verify-email", post(post_verify_email::post_verify_email))
        .route(
            "/verify-email/resend",
            post(post_resend_verification_email::post_resend_verification_email),
        )
        .route("/mfa/setup", get(get_mfa_setup::get_mfa_setup))
        .route(
            "/mfa/setup/confirm",
            post(post_mfa_setup_confirm::post_mfa_setup_confirm),
        )
        .route("/sign-in", post(post_sign_in::post_sign_in))
        .route("/mfa/verify", post(post_mfa_verify::post_mfa_verify))
        .route("/refresh", post(post_refresh::post_refresh))
        .route("/sign-out", post(post_sign_out::post_sign_out))
        .route(
            "/sign-out-everywhere",
            post(post_sign_out_everywhere::post_sign_out_everywhere),
        )
        .route(
            "/password-reset",
            post(post_password_reset::post_password_reset),
        )
        .route(
            "/password-reset/complete",
            post(post_password_reset_complete::post_password_reset_complete),
        )
        .route("/session", get(get_session::get_session))
}
