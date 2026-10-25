use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, build_test_app, cookie_pair, find_set_cookie, get_with_cookies,
    next_totp_code, post_json_with_cookies, recorded_event_types, response_json,
    session_cookie_header, set_cookie_headers, sign_in, sign_in_to_code_step,
    sign_up_with_authenticator, verify_code,
};

struct RememberedSignIn {
    session_cookies: String,
    device_cookie: String,
}

async fn sign_in_remembering_the_device(
    db_pool: &PgPool,
    email: &str,
    totp_secret: &[u8],
) -> RememberedSignIn {
    let challenge_cookie = sign_in_to_code_step(db_pool, email).await;
    let verify_response = verify_code(
        db_pool,
        &challenge_cookie,
        &next_totp_code(totp_secret),
        true,
    )
    .await;
    assert_eq!(verify_response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&verify_response);
    RememberedSignIn {
        session_cookies: session_cookie_header(&set_cookies),
        device_cookie: cookie_pair(&find_set_cookie(&set_cookies, "__Host-daibx_device")),
    }
}

async fn post_with_cookies(
    db_pool: &PgPool,
    request_path: &str,
    cookie_header: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        request_path,
        json!({}),
        cookie_header,
    )
    .await
}

async fn session_status(db_pool: &PgPool, cookie_header: &str) -> StatusCode {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/session",
        cookie_header,
    )
    .await
    .status()
}

#[sqlx::test]
async fn sign_out_revokes_this_session_and_keeps_the_remembered_device(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let remembered_sign_in = sign_in_remembering_the_device(
        &db_pool,
        &signed_in_person.email,
        &signed_in_person.totp_secret,
    )
    .await;

    let response = post_with_cookies(
        &db_pool,
        "/auth/sign-out",
        &remembered_sign_in.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    for cookie_name in ["__Host-daibx_access", "__Host-daibx_refresh"] {
        assert!(find_set_cookie(&set_cookies, cookie_name).contains("Max-Age=0"));
    }
    assert!(
        !set_cookies
            .iter()
            .any(|set_cookie| set_cookie.starts_with("__Host-daibx_device=")),
        "sign-out keeps the remembered device"
    );
    assert_eq!(
        response_json(response).await,
        json!({ "status": "signed_out" })
    );
    assert_eq!(
        session_status(&db_pool, &remembered_sign_in.session_cookies).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_with_cookies(
            &db_pool,
            "/auth/refresh",
            &remembered_sign_in.session_cookies
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        session_status(&db_pool, &signed_in_person.session_cookies).await,
        StatusCode::OK,
        "only this session ends"
    );
    let device_sign_in = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &remembered_sign_in.device_cookie,
    )
    .await;
    assert_eq!(
        response_json(device_sign_in).await,
        json!({ "next": "signed_in" })
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SessionRevoked)
    );
}

#[sqlx::test]
async fn sign_out_returns_401_without_a_session(db_pool: PgPool) {
    for request_path in ["/auth/sign-out", "/account/sessions/end-all"] {
        assert_eq!(
            post_with_cookies(&db_pool, request_path, "").await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
