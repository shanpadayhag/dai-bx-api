use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::calendar_events::create::post_event;
use crate::calendar_events::list::list_events;
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

const NOV_9: &str = "2026-11-09T09:00:00.000Z";
const NOV_16: &str = "2026-11-16T09:00:00.000Z";
const NOV_23: &str = "2026-11-23T09:00:00.000Z";
const NOV_30: &str = "2026-11-30T09:00:00.000Z";

async fn create_weekly_gym(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    ends: serde_json::Value,
) -> String {
    let response = post_event(
        db_pool,
        signed_in_person,
        json!({
            "title": "Gym",
            "start": "2026-11-09T09:00",
            "end": "2026-11-09T10:00",
            "inputZone": "UTC",
            "repeat": { "frequency": "weekly", "interval": 1, "weekdays": ["mon"], "ends": ends },
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["series"]["id"]
        .as_str()
        .expect("a series id")
        .to_owned()
}

fn occurrence_path(series_id: &str, original_start: &str) -> String {
    format!("/calendar/series/{series_id}/occurrences/{original_start}")
}

async fn patch_occurrence(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    series_id: &str,
    original_start: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &occurrence_path(series_id, original_start),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

async fn change_occurrence(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    series_id: &str,
    original_start: &str,
    request_body: serde_json::Value,
) -> serde_json::Value {
    let response = patch_occurrence(
        db_pool,
        signed_in_person,
        series_id,
        original_start,
        request_body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn delete_occurrence(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    series_id: &str,
    original_start: &str,
    scope_query: &str,
) -> axum::response::Response {
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!(
            "{}?{scope_query}",
            occurrence_path(series_id, original_start)
        ),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn week_of(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    monday: &str,
) -> Vec<(String, String)> {
    list_events(
        db_pool,
        signed_in_person,
        &format!("start={monday}&days=7&timeZone=UTC"),
    )
    .await
    .iter()
    .map(|event_json| {
        (
            event_json["title"].as_str().expect("a title").to_owned(),
            event_json["startsAt"].as_str().expect("a start").to_owned(),
        )
    })
    .collect()
}

fn shown(title: &str, starts_at: &str) -> (String, String) {
    (title.to_owned(), starts_at.to_owned())
}

#[sqlx::test]
async fn moves_only_this_occurrence(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;

    let moved_occurrence = change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        json!({
            "scope": "this",
            "start": "2026-11-17T10:00",
            "end": "2026-11-17T11:00",
            "inputZone": "UTC",
        }),
    )
    .await;

    assert_eq!(moved_occurrence["id"], format!("{series_id}:{NOV_16}"));
    assert_eq!(moved_occurrence["startsAt"], "2026-11-17T10:00:00.000Z");
    assert_eq!(moved_occurrence["series"]["originalStart"], NOV_16);
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Gym", "2026-11-17T10:00:00.000Z")]
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-23").await,
        vec![shown("Gym", NOV_23)]
    );
}

#[sqlx::test]
async fn edits_a_changed_occurrence_again(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;
    change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        json!({ "scope": "this", "title": "Swim" }),
    )
    .await;

    let edited_again = change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        json!({
            "scope": "this",
            "start": "2026-11-16T12:00",
            "end": "2026-11-16T13:00",
            "inputZone": "UTC",
        }),
    )
    .await;

    assert_eq!(edited_again["id"], format!("{series_id}:{NOV_16}"));
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Swim", "2026-11-16T12:00:00.000Z")]
    );
}

#[sqlx::test]
async fn deletes_only_this_occurrence(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;
    change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_23,
        json!({ "scope": "this", "title": "Swim" }),
    )
    .await;

    for original_start in [NOV_16, NOV_23] {
        let response = delete_occurrence(
            &db_pool,
            &signed_in_person,
            &series_id,
            original_start,
            "scope=this",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{original_start}");
        assert_eq!(response_json(response).await["status"], "deleted");
    }
    let deleted_again = delete_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        "scope=this",
    )
    .await;

    assert_eq!(deleted_again.status(), StatusCode::NOT_FOUND);
    assert!(
        week_of(&db_pool, &signed_in_person, "2026-11-16")
            .await
            .is_empty()
    );
    assert!(
        week_of(&db_pool, &signed_in_person, "2026-11-23")
            .await
            .is_empty()
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-30").await,
        vec![shown("Gym", NOV_30)]
    );
}

#[sqlx::test]
async fn changes_this_and_the_following_occurrences(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;

    let first_changed = change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_23,
        json!({
            "scope": "following",
            "title": "Late gym",
            "start": "2026-11-23T11:00",
            "end": "2026-11-23T12:00",
            "inputZone": "UTC",
        }),
    )
    .await;

    assert_eq!(first_changed["startsAt"], "2026-11-23T11:00:00.000Z");
    assert_ne!(first_changed["series"]["id"], series_id.as_str());
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Gym", NOV_16)]
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-23").await,
        vec![shown("Late gym", "2026-11-23T11:00:00.000Z")]
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-30").await,
        vec![shown("Late gym", "2026-11-30T11:00:00.000Z")]
    );
}

#[sqlx::test]
async fn splits_a_counted_series_without_adding_occurrences(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "after": 4 })).await;

    change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_23,
        json!({ "scope": "following", "title": "Swim" }),
    )
    .await;

    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Gym", NOV_16)]
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-30").await,
        vec![shown("Swim", NOV_30)]
    );
    assert!(
        week_of(&db_pool, &signed_in_person, "2026-12-07")
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn deletes_this_and_the_following_occurrences(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;
    change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_30,
        json!({ "scope": "this", "title": "Swim" }),
    )
    .await;

    let response = delete_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_23,
        "scope=following",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Gym", NOV_16)]
    );
    for later_monday in ["2026-11-23", "2026-11-30", "2026-12-07"] {
        assert!(
            week_of(&db_pool, &signed_in_person, later_monday)
                .await
                .is_empty(),
            "{later_monday}"
        );
    }
}

#[sqlx::test]
async fn deleting_the_first_occurrence_and_the_following_ones_removes_the_series(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;

    let response = delete_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_9,
        "scope=following",
    )
    .await;
    let later_change = patch_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        json!({ "scope": "this", "title": "Swim" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(later_change.status(), StatusCode::NOT_FOUND);
    assert!(
        week_of(&db_pool, &signed_in_person, "2026-11-09")
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn renaming_all_occurrences_keeps_the_changed_and_deleted_ones(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;
    change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        json!({
            "scope": "this",
            "start": "2026-11-18T10:00",
            "end": "2026-11-18T11:00",
            "inputZone": "UTC",
        }),
    )
    .await;
    delete_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_23,
        "scope=this",
    )
    .await;

    let renamed_occurrence = change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_30,
        json!({ "scope": "all", "title": "Swim" }),
    )
    .await;

    assert_eq!(renamed_occurrence["title"], "Swim");
    assert_eq!(renamed_occurrence["startsAt"], NOV_30);
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-09").await,
        vec![shown("Swim", NOV_9)]
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Gym", "2026-11-18T10:00:00.000Z")]
    );
    assert!(
        week_of(&db_pool, &signed_in_person, "2026-11-23")
            .await
            .is_empty()
    );
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-30").await,
        vec![shown("Swim", NOV_30)]
    );
}

#[sqlx::test]
async fn moving_all_occurrences_to_another_weekday_drops_the_changed_and_deleted_ones(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;
    change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_16,
        json!({
            "scope": "this",
            "start": "2026-11-18T10:00",
            "end": "2026-11-18T11:00",
            "inputZone": "UTC",
        }),
    )
    .await;
    delete_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_23,
        "scope=this",
    )
    .await;

    let moved_occurrence = change_occurrence(
        &db_pool,
        &signed_in_person,
        &series_id,
        NOV_30,
        json!({
            "scope": "all",
            "start": "2026-12-01T09:00",
            "end": "2026-12-01T10:00",
            "inputZone": "UTC",
        }),
    )
    .await;

    assert_eq!(moved_occurrence["startsAt"], "2026-12-01T09:00:00.000Z");
    assert_eq!(
        moved_occurrence["series"]["repeat"]["weekdays"],
        json!(["tue"])
    );
    for (monday, tuesday_start) in [
        ("2026-11-09", "2026-11-10T09:00:00.000Z"),
        ("2026-11-16", "2026-11-17T09:00:00.000Z"),
        ("2026-11-23", "2026-11-24T09:00:00.000Z"),
    ] {
        assert_eq!(
            week_of(&db_pool, &signed_in_person, monday).await,
            vec![shown("Gym", tuesday_start)],
            "{monday}"
        );
    }
}

#[sqlx::test]
async fn deletes_all_occurrences(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;

    let response =
        delete_occurrence(&db_pool, &signed_in_person, &series_id, NOV_23, "scope=all").await;

    assert_eq!(response.status(), StatusCode::OK);
    for monday in ["2026-11-09", "2026-11-23"] {
        assert!(
            week_of(&db_pool, &signed_in_person, monday)
                .await
                .is_empty(),
            "{monday}"
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_an_occurrence_that_is_not_there(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;
    let their_series_id =
        create_weekly_gym(&db_pool, &other_person, json!({ "never": true })).await;
    let unknown_series_id = Uuid::now_v7().to_string();

    for (case_description, wanted_series_id, original_start) in [
        (
            "a day the rule skips",
            series_id.as_str(),
            "2026-11-10T09:00:00.000Z",
        ),
        (
            "a time the rule skips",
            series_id.as_str(),
            "2026-11-16T09:30:00.000Z",
        ),
        (
            "before the series starts",
            series_id.as_str(),
            "2026-11-02T09:00:00.000Z",
        ),
        ("a bad start", series_id.as_str(), "yesterday"),
        ("someone else's series", their_series_id.as_str(), NOV_16),
        ("an unknown series", unknown_series_id.as_str(), NOV_16),
        ("a bad series id", "not-an-id", NOV_16),
    ] {
        let patch_response = patch_occurrence(
            &db_pool,
            &signed_in_person,
            wanted_series_id,
            original_start,
            json!({ "scope": "this", "title": "Swim" }),
        )
        .await;
        let delete_response = delete_occurrence(
            &db_pool,
            &signed_in_person,
            wanted_series_id,
            original_start,
            "scope=all",
        )
        .await;
        assert_eq!(
            patch_response.status(),
            StatusCode::NOT_FOUND,
            "patch {case_description}"
        );
        assert_eq!(
            delete_response.status(),
            StatusCode::NOT_FOUND,
            "delete {case_description}"
        );
    }
    assert_eq!(
        week_of(&db_pool, &other_person, "2026-11-16").await,
        vec![shown("Gym", NOV_16)]
    );
}

#[sqlx::test]
async fn refuses_a_bad_scope_or_a_repeat_on_one_occurrence(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_weekly_gym(&db_pool, &signed_in_person, json!({ "never": true })).await;

    for refused_body in [
        json!({ "scope": "some", "title": "Swim" }),
        json!({ "title": "Swim" }),
        json!({
            "scope": "this",
            "inputZone": "UTC",
            "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
        }),
    ] {
        let response = patch_occurrence(
            &db_pool,
            &signed_in_person,
            &series_id,
            NOV_16,
            refused_body.clone(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_body}"
        );
    }
    for refused_query in ["scope=some", "scope=", "other=this"] {
        let response = delete_occurrence(
            &db_pool,
            &signed_in_person,
            &series_id,
            NOV_16,
            refused_query,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_query}"
        );
    }
    assert_eq!(
        week_of(&db_pool, &signed_in_person, "2026-11-16").await,
        vec![shown("Gym", NOV_16)]
    );
}
