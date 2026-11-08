use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::card_schedules::create::{
    board_with_review_card, create_schedule, merged, once_at, read_card, schedule_id, weekly_review,
};
use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn patch_schedule(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{schedule_id}"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

#[sqlx::test]
async fn changes_a_schedule_s_target_time_and_repeat(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    let response = patch_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        json!({
            "kind": "repeating",
            "targetListId": scheduled_card.doing_list_id,
            "start": "2030-01-08T07:00",
            "inputZone": "UTC",
            "repeat": { "frequency": "weekly", "interval": 2, "weekdays": ["tue"], "ends": { "after": 3 } },
            "block": { "minutes": 30 },
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let card_schedules = response_json(response).await["schedules"].clone();
    let changed_schedule = &card_schedules[0];
    assert_eq!(changed_schedule["id"], weekly_schedule["id"]);
    assert_eq!(
        changed_schedule["targetListId"],
        scheduled_card.doing_list_id
    );
    assert_eq!(changed_schedule["targetListName"], "Doing");
    assert_eq!(changed_schedule["start"], "2030-01-08T07:00");
    assert_eq!(changed_schedule["timeZone"], "UTC");
    assert_eq!(changed_schedule["repeat"]["interval"], 2);
    assert_eq!(
        changed_schedule["block"],
        json!({ "minutes": 30, "categoryId": null })
    );
    assert_eq!(changed_schedule["nextRunAt"], "2030-01-08T07:00:00.000Z");
    assert_eq!(
        read_card(&db_pool, &signed_in_person, &scheduled_card.card_id).await["schedules"],
        card_schedules
    );
}

#[sqlx::test]
async fn turns_a_repeating_schedule_into_a_once_schedule(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    let response = patch_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        once_at(&scheduled_card.to_do_list_id, "2030-02-01T12:00"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let changed_schedule = response_json(response).await["schedules"][0].clone();
    assert_eq!(changed_schedule["kind"], "once");
    assert_eq!(changed_schedule["repeat"], serde_json::Value::Null);
    assert_eq!(changed_schedule["nextRunAt"], "2030-02-01T12:00:00.000Z");
}

#[sqlx::test]
async fn refuses_a_bad_change_and_answers_not_found_for_someone_else_s_schedule(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let other_board = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    let to_do = scheduled_card.to_do_list_id.as_str();

    for (refused_body, expected_status) in [
        (
            once_at(to_do, "2026-01-01T09:00"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            merged(
                once_at(to_do, "2030-01-07T09:00"),
                json!({ "kind": "never" }),
            ),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            once_at(&other_board.to_do_list_id, "2030-01-07T09:00"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = patch_schedule(
            &db_pool,
            &signed_in_person,
            schedule_id(&weekly_schedule),
            refused_body.clone(),
        )
        .await;
        assert_eq!(response.status(), expected_status, "{refused_body}");
    }
    for (wanted_schedule_id, signed_in) in [
        (schedule_id(&weekly_schedule).to_owned(), &other_person),
        (uuid::Uuid::now_v7().to_string(), &signed_in_person),
        (String::from("not-a-uuid"), &signed_in_person),
    ] {
        let response = patch_schedule(
            &db_pool,
            signed_in,
            &wanted_schedule_id,
            once_at(to_do, "2030-01-07T09:00"),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{wanted_schedule_id}"
        );
    }
    assert_eq!(
        read_card(&db_pool, &signed_in_person, &scheduled_card.card_id).await["schedules"],
        json!([weekly_schedule])
    );
}
