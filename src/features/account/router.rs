use axum::Router;
use axum::routing::{get, post};

use crate::core::state::AppState;
use crate::features::account::handlers::{
    get_account, get_account_sessions, patch_account, post_account_authenticator,
    post_account_authenticator_confirm, post_account_delete, post_account_email,
    post_account_email_confirm, post_account_password, post_account_recovery_codes,
    post_account_sessions_end, post_account_sessions_end_all,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(get_account::get_account).patch(patch_account::patch_account),
        )
        .route(
            "/password",
            post(post_account_password::post_account_password),
        )
        .route("/email", post(post_account_email::post_account_email))
        .route(
            "/email/confirm",
            post(post_account_email_confirm::post_account_email_confirm),
        )
        .route(
            "/recovery-codes",
            post(post_account_recovery_codes::post_account_recovery_codes),
        )
        .route(
            "/authenticator",
            post(post_account_authenticator::post_account_authenticator),
        )
        .route(
            "/authenticator/confirm",
            post(post_account_authenticator_confirm::post_account_authenticator_confirm),
        )
        .route("/sessions", get(get_account_sessions::get_account_sessions))
        .route(
            "/sessions/end",
            post(post_account_sessions_end::post_account_sessions_end),
        )
        .route(
            "/sessions/end-all",
            post(post_account_sessions_end_all::post_account_sessions_end_all),
        )
        .route("/delete", post(post_account_delete::post_account_delete))
}
