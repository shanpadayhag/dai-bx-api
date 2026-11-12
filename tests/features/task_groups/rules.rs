use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, post_json_with_cookies, response_json,
    send, sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, task_id_named};

fn every_day() -> serde_json::Value {
    json!({ "frequency": "daily", "interval": 1, "ends": { "never": true } })
}

fn weekends() -> serde_json::Value {
    json!({ "frequency": "weekly", "interval": 1, "weekdays": ["sat", "sun"], "ends": { "never": true } })
}

async fn list_groups_in_zone(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    time_zone_name: &str,
) -> Vec<serde_json::Value> {
    let request = Request::get("/task-groups")
        .header(header::COOKIE, &signed_in_person.session_cookies)
        .header("x-time-zone", time_zone_name)
        .body(Body::empty())
        .expect("test request should build");
    let response = send(build_test_app(db_pool.clone()), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response)
        .await
        .as_array()
        .expect("a list of groups")
        .clone()
}

async fn patch_group(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    group_id: &str,
    request_body: serde_json::Value,
) -> axum::http::Response<Body> {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/task-groups/{group_id}"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

fn group_id(group: &serde_json::Value) -> String {
    group["id"].as_str().expect("an id").to_owned()
}

fn wall_clock_in(instant_value: &serde_json::Value, time_zone_name: &str) -> (i8, i8) {
    let instant: Timestamp = instant_value
        .as_str()
        .expect("an instant")
        .parse()
        .expect("the instant should parse");
    let zoned_instant =
        instant.to_zoned(TimeZone::get(time_zone_name).expect("the zone should exist"));
    (zoned_instant.hour(), zoned_instant.minute())
}

#[sqlx::test]
async fn creates_a_group_with_rules_and_reads_them_back(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let created_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Night",
            "shows": { "repeat": weekends(), "windowStart": "22:00", "windowEnd": "02:00" },
            "unchecks": { "repeat": weekends(), "time": "04:00" }
        }),
    )
    .await;

    assert_eq!(
        created_group["shows"],
        json!({ "repeat": weekends(), "windowStart": "22:00", "windowEnd": "02:00" })
    );
    assert_eq!(
        created_group["unchecks"],
        json!({ "repeat": weekends(), "time": "04:00" })
    );
    assert!(created_group["timetable"]["nextShowsAt"].is_string());
    assert!(created_group["timetable"]["nextUncheckAt"].is_string());
}

#[sqlx::test]
async fn changing_only_the_name_keeps_the_rules_and_null_clears_them(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let weekend_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Weekend",
            "shows": { "repeat": weekends() },
            "unchecks": { "repeat": weekends(), "time": "00:00" }
        }),
    )
    .await;

    let renamed_response = patch_group(
        &db_pool,
        &signed_in_person,
        &group_id(&weekend_group),
        json!({ "name": "Saturday and Sunday" }),
    )
    .await;
    assert_eq!(renamed_response.status(), StatusCode::OK);
    let renamed_group = response_json(renamed_response).await;
    let cleared_response = patch_group(
        &db_pool,
        &signed_in_person,
        &group_id(&weekend_group),
        json!({ "name": "Saturday and Sunday", "shows": null, "unchecks": null }),
    )
    .await;
    assert_eq!(cleared_response.status(), StatusCode::OK);
    let cleared_group = response_json(cleared_response).await;

    assert_eq!(renamed_group["name"], "Saturday and Sunday");
    assert_eq!(
        renamed_group["shows"],
        json!({ "repeat": weekends(), "windowStart": null, "windowEnd": null })
    );
    assert_eq!(renamed_group["unchecks"]["time"], "00:00");
    assert_eq!(cleared_group["shows"], serde_json::Value::Null);
    assert_eq!(cleared_group["unchecks"], serde_json::Value::Null);
    assert_eq!(cleared_group["timetable"]["dueNow"], true);
}

#[sqlx::test]
async fn refuses_bad_rules(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    for refused_body in [
        json!({ "name": "Today", "shows": { "repeat": every_day(), "windowStart": "06:00" } }),
        json!({ "name": "Today", "shows": { "repeat": every_day(), "windowStart": "06:00", "windowEnd": "06:00" } }),
        json!({ "name": "Today", "unchecks": { "repeat": every_day(), "time": "4am" } }),
        json!({ "name": "Today", "unchecks": { "repeat": { "frequency": "weekly", "interval": 1, "ends": { "never": true } }, "time": "04:00" } }),
    ] {
        let response = patch_group(
            &db_pool,
            &signed_in_person,
            &group_id(&today_group),
            refused_body.clone(),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_body}"
        );
    }
    let create_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/task-groups",
        json!({ "name": "Bad", "unchecks": { "repeat": every_day(), "time": "25:00" } }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(create_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn answers_not_found_when_setting_rules_on_someone_else_s_group(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group = create_group(&db_pool, &owner, json!({ "name": "Mine" })).await;

    let response = patch_group(
        &db_pool,
        &other_person,
        &group_id(&owner_group),
        json!({ "name": "Mine", "shows": { "repeat": every_day() } }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn works_the_timetable_out_in_the_device_zone_when_no_calendar_zone_is_stored(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    create_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Daily",
            "shows": { "repeat": every_day() },
            "unchecks": { "repeat": every_day(), "time": "00:00" }
        }),
    )
    .await;

    let listed_groups = list_groups_in_zone(&db_pool, &signed_in_person, "Asia/Manila").await;

    let daily_timetable = &listed_groups[0]["timetable"];
    assert_eq!(daily_timetable["dueNow"], true);
    assert_eq!(
        wall_clock_in(&daily_timetable["dueUntil"], "Asia/Manila"),
        (0, 0)
    );
    assert_eq!(
        wall_clock_in(&daily_timetable["nextUncheckAt"], "Asia/Manila"),
        (0, 0)
    );
}

#[sqlx::test]
async fn a_tick_from_before_the_last_uncheck_counts_as_open(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let daily_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "Daily", "unchecks": { "repeat": every_day(), "time": "00:00" } }),
    )
    .await;
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &group_id(&daily_group),
        "Stretch",
        None,
    )
    .await;
    let stretch_task_id = task_id_named(&group_tasks, "Stretch");
    set_task_done(&db_pool, &signed_in_person, &stretch_task_id, true).await;

    let fresh_groups = list_groups_in_zone(&db_pool, &signed_in_person, "Asia/Manila").await;
    sqlx::query("UPDATE tasks SET completed_at = now() - interval '2 days'")
        .execute(&db_pool)
        .await
        .expect("moving the tick back should run");
    let later_groups = list_groups_in_zone(&db_pool, &signed_in_person, "Asia/Manila").await;

    assert_eq!(fresh_groups[0]["openTaskCount"], 0);
    assert_eq!(later_groups[0]["openTaskCount"], 1);
}
