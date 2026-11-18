use axum::body::Body;
use axum::http::{Response, StatusCode};
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, cookie_pair, count_known_browsers,
    count_pending_sign_in_alerts, find_set_cookie, next_totp_code, set_cookie_headers, sign_in,
    sign_in_to_code_step, sign_up_with_authenticator, verify_code,
};

async fn pending_alert_details(db_pool: &PgPool) -> Vec<(String, Option<String>, bool)> {
    sqlx::query_as(
        "SELECT sign_in_method, user_agent, signed_in_at > now() - interval '1 minute'
         FROM pending_sign_in_alerts",
    )
    .fetch_all(db_pool)
    .await
    .expect("listing pending alerts should succeed")
}

async fn sign_in_with_code_from_browser(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    browser_cookie: &str,
    entered_code: &str,
) -> Response<Body> {
    let challenge_cookie = sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    verify_code(
        db_pool,
        &format!("{challenge_cookie}; {browser_cookie}"),
        entered_code,
        false,
    )
    .await
}

#[sqlx::test]
async fn first_authenticator_setup_queues_no_alert(db_pool: PgPool) {
    sign_up_with_authenticator(&db_pool).await;

    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
}

#[sqlx::test]
async fn a_code_sign_in_from_a_new_browser_queues_one_alert(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        "",
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        pending_alert_details(&db_pool).await,
        vec![(
            String::from("authenticator"),
            Some(String::from("DaiBX feature test")),
            true,
        )]
    );
}

#[sqlx::test]
async fn a_code_sign_in_from_a_known_browser_queues_no_alert(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        &signed_in_person.browser_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
}

#[sqlx::test]
async fn a_recovery_code_sign_in_queues_an_alert_even_from_a_known_browser(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        &signed_in_person.browser_cookie,
        &signed_in_person.recovery_codes[0],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        pending_alert_details(&db_pool).await[0].0,
        "recovery_code",
        "the alert records how the person signed in"
    );
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 1);
}

#[sqlx::test]
async fn a_remembered_device_sign_in_queues_no_alert(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let remembered_response = verify_code(
        &db_pool,
        &format!("{challenge_cookie}; {}", signed_in_person.browser_cookie),
        &next_totp_code(&signed_in_person.totp_secret),
        true,
    )
    .await;
    let device_cookie = cookie_pair(&find_set_cookie(
        &set_cookie_headers(&remembered_response),
        "__Host-daibx_device",
    ));

    let response = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &device_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
}

#[sqlx::test]
async fn a_refused_code_queues_no_alert(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = sign_in_with_code_from_browser(&db_pool, &signed_in_person, "", "000000").await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
}

#[sqlx::test]
async fn a_sign_in_that_rolls_back_queues_nothing(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    sqlx::raw_sql(
        "CREATE FUNCTION refuse_sign_in_event() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'refused for this test'; END $$;
         CREATE TRIGGER refuse_sign_in_event BEFORE INSERT ON security_events
         FOR EACH ROW WHEN (NEW.event_type = 'sign_in_succeeded')
         EXECUTE FUNCTION refuse_sign_in_event();",
    )
    .execute(&db_pool)
    .await
    .expect("adding the trigger should succeed");

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        "",
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
    assert_eq!(count_known_browsers(&db_pool).await, 1);
}
