use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::list_id;
use crate::cards::create::{board_with_cards, card_id};
use crate::categories::update::find_category_id;
use crate::support::{
    SignedInPerson, build_test_app, get, get_with_cookies, post_json, post_json_with_cookies,
    response_json, sign_up_with_authenticator,
};

pub const FIRST_MONDAY: &str = "2030-01-07T10:00:00.000Z";

pub struct ScheduledCard {
    pub board: serde_json::Value,
    pub card_id: String,
    pub to_do_list_id: String,
    pub doing_list_id: String,
}

pub async fn board_with_review_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> ScheduledCard {
    let board = board_with_cards(
        db_pool,
        signed_in_person,
        &[("To Do", &[]), ("Doing", &["Weekly review"])],
    )
    .await;
    ScheduledCard {
        card_id: card_id(&board, "Weekly review").to_owned(),
        to_do_list_id: list_id(&board, "To Do").to_owned(),
        doing_list_id: list_id(&board, "Doing").to_owned(),
        board,
    }
}

pub fn weekly_on_monday() -> serde_json::Value {
    json!({ "frequency": "weekly", "interval": 1, "weekdays": ["mon"], "ends": { "never": true } })
}

pub fn weekly_review(target_list_id: &str) -> serde_json::Value {
    json!({
        "kind": "repeating",
        "targetListId": target_list_id,
        "start": "2030-01-07T18:00",
        "inputZone": "Asia/Manila",
        "repeat": weekly_on_monday(),
        "block": null,
    })
}

pub fn once_at(target_list_id: &str, start: &str) -> serde_json::Value {
    json!({
        "kind": "once",
        "targetListId": target_list_id,
        "start": start,
        "inputZone": "UTC",
    })
}

pub fn merged(base_body: serde_json::Value, extra_fields: serde_json::Value) -> serde_json::Value {
    let mut merged_body = base_body;
    if let (Some(merged_fields), Some(extra_fields)) =
        (merged_body.as_object_mut(), extra_fields.as_object())
    {
        merged_fields.extend(extra_fields.clone());
    }
    merged_body
}

pub async fn post_schedule(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/schedules"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn create_schedule(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    request_body: serde_json::Value,
) -> serde_json::Value {
    let response = post_schedule(db_pool, signed_in_person, card_id, request_body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let card_schedules = response_json(response).await["schedules"].clone();
    card_schedules
        .as_array()
        .and_then(|schedules| schedules.last())
        .expect("the new schedule should be listed")
        .clone()
}

pub fn schedule_id(schedule_json: &serde_json::Value) -> &str {
    schedule_json["id"].as_str().expect("a schedule id")
}

pub async fn read_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn creates_a_repeating_schedule_that_shows_on_the_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;

    let response = post_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let card_schedules = response_json(response).await["schedules"].clone();
    let new_schedule = &card_schedules[0];
    assert!(new_schedule["id"].is_string());
    assert_eq!(
        new_schedule,
        &json!({
            "id": new_schedule["id"],
            "kind": "repeating",
            "targetListId": scheduled_card.to_do_list_id,
            "targetListName": "To Do",
            "start": "2030-01-07T18:00",
            "startsAt": FIRST_MONDAY,
            "timeZone": "Asia/Manila",
            "repeat": weekly_on_monday(),
            "block": null,
            "paused": false,
            "nextRunAt": FIRST_MONDAY,
        })
    );
    assert_eq!(
        read_card(&db_pool, &signed_in_person, &scheduled_card.card_id).await["schedules"],
        card_schedules
    );
}

#[sqlx::test]
async fn creates_a_once_schedule_in_its_own_zone_with_a_block(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;

    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        json!({
            "kind": "once",
            "targetListId": scheduled_card.doing_list_id,
            "start": "2030-03-01T09:30",
            "inputZone": "Asia/Manila",
            "timeZone": "Europe/London",
            "block": { "minutes": 45, "categoryId": studies_category_id },
        }),
    )
    .await;

    assert_eq!(once_schedule["kind"], "once");
    assert_eq!(once_schedule["start"], "2030-03-01T01:30");
    assert_eq!(once_schedule["startsAt"], "2030-03-01T01:30:00.000Z");
    assert_eq!(once_schedule["timeZone"], "Europe/London");
    assert_eq!(once_schedule["repeat"], serde_json::Value::Null);
    assert_eq!(
        once_schedule["block"],
        json!({ "minutes": 45, "categoryId": studies_category_id })
    );
    assert_eq!(once_schedule["nextRunAt"], "2030-03-01T01:30:00.000Z");
}

#[sqlx::test]
async fn runs_a_repeat_that_started_in_the_past_from_its_next_future_occurrence(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let before_create = now_millis();

    let daily_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        json!({
            "kind": "repeating",
            "targetListId": scheduled_card.to_do_list_id,
            "start": "2026-01-05T09:00",
            "inputZone": "UTC",
            "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
        }),
    )
    .await;

    let next_run_millis = iso_millis(daily_schedule["nextRunAt"].as_str().expect("a next run"));
    assert_eq!(daily_schedule["start"], "2026-01-05T09:00");
    assert!(next_run_millis > before_create);
    assert!(next_run_millis <= before_create + 86_400_000);
    assert!(
        daily_schedule["nextRunAt"]
            .as_str()
            .is_some_and(|next_run| next_run.ends_with("T09:00:00.000Z"))
    );
}

pub fn now_millis() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_millis(),
    )
    .expect("millis fit")
}

pub fn iso_millis(instant_text: &str) -> i64 {
    instant_text
        .parse::<jiff::Timestamp>()
        .expect("an ISO instant")
        .as_millisecond()
}

#[sqlx::test]
async fn refuses_a_schedule_that_can_never_run_or_is_badly_formed(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let to_do = scheduled_card.to_do_list_id.as_str();

    for (rule_description, refused_body) in [
        (
            "a once start in the past",
            once_at(to_do, "2026-01-01T09:00"),
        ),
        (
            "a repeat that already ended",
            json!({
                "kind": "repeating",
                "targetListId": to_do,
                "start": "2026-01-05T09:00",
                "inputZone": "UTC",
                "repeat": { "frequency": "daily", "interval": 1, "ends": { "onDate": "2026-02-01" } },
            }),
        ),
        (
            "a repeating schedule without a repeat",
            json!({ "kind": "repeating", "targetListId": to_do, "start": "2030-01-07T09:00", "inputZone": "UTC" }),
        ),
        (
            "a once schedule with a repeat",
            merged(
                once_at(to_do, "2030-01-07T09:00"),
                json!({ "repeat": weekly_on_monday() }),
            ),
        ),
        (
            "an unknown kind",
            json!({ "kind": "sometimes", "targetListId": to_do, "start": "2030-01-07T09:00", "inputZone": "UTC" }),
        ),
        (
            "a block under 15 minutes",
            merged(
                once_at(to_do, "2030-01-07T09:00"),
                json!({ "block": { "minutes": 10 } }),
            ),
        ),
        (
            "a missing input zone",
            json!({ "kind": "once", "targetListId": to_do, "start": "2030-01-07T09:00" }),
        ),
        (
            "an unknown zone",
            merged(
                once_at(to_do, "2030-01-07T09:00"),
                json!({ "timeZone": "Mars/Olympus" }),
            ),
        ),
        ("a bad start", once_at(to_do, "next Monday")),
    ] {
        let response = post_schedule(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            refused_body,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "should refuse {rule_description}"
        );
    }
    assert_eq!(
        read_card(&db_pool, &signed_in_person, &scheduled_card.card_id).await["schedules"],
        json!([])
    );
}

#[sqlx::test]
async fn refuses_the_21st_schedule_on_a_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    for day_of_month in 1..=20 {
        create_schedule(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            once_at(
                &scheduled_card.to_do_list_id,
                &format!("2030-01-{day_of_month:02}T09:00"),
            ),
        )
        .await;
    }

    let response = post_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.to_do_list_id, "2030-01-21T09:00"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn answers_not_found_for_a_list_card_or_category_that_is_not_theirs(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let other_board = board_with_review_card(&db_pool, &signed_in_person).await;
    let their_card = board_with_review_card(&db_pool, &other_person).await;
    let their_category_id = find_category_id(&db_pool, &other_person, "Studies").await;
    let my_card_id = scheduled_card.card_id.as_str();
    let to_do = scheduled_card.to_do_list_id.as_str();

    for (case_description, wanted_card_id, refused_body) in [
        (
            "a list on another board",
            my_card_id,
            once_at(&other_board.to_do_list_id, "2030-01-07T09:00"),
        ),
        (
            "someone else's list",
            my_card_id,
            once_at(&their_card.to_do_list_id, "2030-01-07T09:00"),
        ),
        (
            "an unknown list",
            my_card_id,
            once_at(&uuid::Uuid::now_v7().to_string(), "2030-01-07T09:00"),
        ),
        (
            "someone else's category",
            my_card_id,
            merged(
                once_at(to_do, "2030-01-07T09:00"),
                json!({ "block": { "minutes": 30, "categoryId": their_category_id } }),
            ),
        ),
        (
            "someone else's card",
            their_card.card_id.as_str(),
            once_at(&their_card.to_do_list_id, "2030-01-07T09:00"),
        ),
        (
            "a bad card id",
            "not-a-uuid",
            once_at(to_do, "2030-01-07T09:00"),
        ),
    ] {
        let response =
            post_schedule(&db_pool, &signed_in_person, wanted_card_id, refused_body).await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{case_description}"
        );
    }
    assert_eq!(
        read_card(&db_pool, &other_person, &their_card.card_id).await["schedules"],
        json!([])
    );
}

#[sqlx::test]
async fn schedules_need_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;

    let response = post_json(
        build_test_app(db_pool.clone()),
        &format!("/cards/{}/schedules", scheduled_card.card_id),
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    let card_response = get(
        build_test_app(db_pool),
        &format!("/cards/{}", scheduled_card.card_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(card_response.status(), StatusCode::UNAUTHORIZED);
}
