use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, cookie_pair, find_set_cookie,
    get_with_cookies, next_totp_code, post_json_with_cookies, recorded_event_types, response_json,
    session_cookie_header, set_cookie_headers, sign_in, sign_in_to_code_step,
    sign_up_with_authenticator, verify_code,
};

const WRONG_PASSWORD: &str = "seven quiet docks drift past a violet lantern";

struct OtherSignIn {
    session_cookies: String,
    device_cookie: String,
}

async fn sign_in_elsewhere(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> OtherSignIn {
    let challenge_cookie = sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    let verify_response = verify_code(
        db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        true,
    )
    .await;
    assert_eq!(verify_response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&verify_response);
    OtherSignIn {
        session_cookies: session_cookie_header(&set_cookies),
        device_cookie: cookie_pair(&find_set_cookie(&set_cookies, "__Host-daibx_device")),
    }
}

async fn list_sessions(db_pool: &PgPool, session_cookies: &str) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/sessions",
        session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn post_account(
    db_pool: &PgPool,
    request_path: &str,
    request_body: serde_json::Value,
    session_cookies: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        request_path,
        request_body,
        session_cookies,
    )
    .await
}

async fn session_status(db_pool: &PgPool, session_cookies: &str) -> StatusCode {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/session",
        session_cookies,
    )
    .await
    .status()
}

#[sqlx::test]
async fn the_session_list_shows_live_sessions_with_this_one_marked(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    sign_in_elsewhere(&db_pool, &signed_in_person).await;

    let listed_sessions = list_sessions(&db_pool, &signed_in_person.session_cookies).await;

    let listed_sessions = listed_sessions["sessions"]
        .as_array()
        .expect("sessions should be a list");
    assert_eq!(listed_sessions.len(), 2);
    assert_eq!(
        listed_sessions
            .iter()
            .filter(|listed_session| listed_session["isCurrent"] == true)
            .count(),
        1
    );
    for listed_session in listed_sessions {
        assert!(listed_session["id"].is_string());
        assert_eq!(listed_session["userAgent"], "DaiBX feature test");
        assert!(
            listed_session["lastSeenAt"]
                .as_str()
                .is_some_and(|last_seen_at| last_seen_at.ends_with('Z'))
        );
        assert!(listed_session["createdAt"].is_string());
    }
}

#[sqlx::test]
async fn ending_another_session_needs_the_password_and_ends_only_that_one(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_sign_in = sign_in_elsewhere(&db_pool, &signed_in_person).await;
    let listed_sessions = list_sessions(&db_pool, &signed_in_person.session_cookies).await;
    let other_session_id = listed_sessions["sessions"]
        .as_array()
        .expect("sessions should be a list")
        .iter()
        .find(|listed_session| listed_session["isCurrent"] == false)
        .and_then(|listed_session| listed_session["id"].as_str())
        .expect("the other session should be listed")
        .to_owned();

    let wrong_password_response = post_account(
        &db_pool,
        "/account/sessions/end",
        json!({ "sessionId": other_session_id, "password": WRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(
        wrong_password_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        session_status(&db_pool, &other_sign_in.session_cookies).await,
        StatusCode::OK
    );

    let response = post_account(
        &db_pool,
        "/account/sessions/end",
        json!({ "sessionId": other_session_id, "password": STRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "session_ended" })
    );
    assert_eq!(
        session_status(&db_pool, &other_sign_in.session_cookies).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        session_status(&db_pool, &signed_in_person.session_cookies).await,
        StatusCode::OK
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SessionRevoked)
    );
}

#[sqlx::test]
async fn ending_this_session_or_an_unknown_one_is_refused(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let current_session_id =
        list_sessions(&db_pool, &signed_in_person.session_cookies).await["sessions"][0]["id"]
            .as_str()
            .expect("the current session should be listed")
            .to_owned();
    let foreign_session_id =
        list_sessions(&db_pool, &other_person.session_cookies).await["sessions"][0]["id"]
            .as_str()
            .expect("the other account's session should be listed")
            .to_owned();

    let current_response = post_account(
        &db_pool,
        "/account/sessions/end",
        json!({ "sessionId": current_session_id, "password": STRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;
    let foreign_response = post_account(
        &db_pool,
        "/account/sessions/end",
        json!({ "sessionId": foreign_session_id, "password": STRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(current_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(current_response).await["error"],
        "use sign out to end this session"
    );
    assert_eq!(foreign_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(foreign_response).await["error"],
        "that session has already ended"
    );
    assert_eq!(
        session_status(&db_pool, &other_person.session_cookies).await,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn ending_every_session_needs_the_password_and_forgets_remembered_devices(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_sign_in = sign_in_elsewhere(&db_pool, &signed_in_person).await;

    let wrong_password_response = post_account(
        &db_pool,
        "/account/sessions/end-all",
        json!({ "password": WRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(
        wrong_password_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let response = post_account(
        &db_pool,
        "/account/sessions/end-all",
        json!({ "password": STRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    for cookie_name in [
        "__Host-daibx_access",
        "__Host-daibx_refresh",
        "__Host-daibx_device",
    ] {
        assert!(find_set_cookie(&set_cookies, cookie_name).contains("Max-Age=0"));
    }
    assert_eq!(
        response_json(response).await,
        json!({ "status": "signed_out" })
    );
    for session_cookies in [
        &signed_in_person.session_cookies,
        &other_sign_in.session_cookies,
    ] {
        assert_eq!(
            session_status(&db_pool, session_cookies).await,
            StatusCode::UNAUTHORIZED
        );
    }
    let device_sign_in = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &other_sign_in.device_cookie,
    )
    .await;
    assert_eq!(
        response_json(device_sign_in).await,
        json!({ "next": "enter_code" })
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SignedOutEverywhere)
    );
}
