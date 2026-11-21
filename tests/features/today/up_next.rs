use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::calendar_events::create::event_id;
use crate::planning::accept::{block, post_accept};
use crate::planning::proposal::{group_with_tasks, planning_person};
use crate::support::{
    SignedInPerson, build_test_app, response_json, send, sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, task_id_named};
use crate::tracking::finish::{minutes_from_now, post_finish};
use crate::tracking::start::start_task;

async fn get_up_next(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    time_zone_name: Option<&str>,
) -> Value {
    let mut request_builder =
        Request::get("/today/up-next").header(header::COOKIE, &signed_in_person.session_cookies);
    if let Some(time_zone_name) = time_zone_name {
        request_builder = request_builder.header("x-time-zone", time_zone_name);
    }
    let request = request_builder
        .body(Body::empty())
        .expect("test request should build");
    let response = send(build_test_app(db_pool.clone()), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn task_names(up_next: &Value) -> Vec<String> {
    up_next["tasks"]
        .as_array()
        .expect("a list of tasks")
        .iter()
        .map(|up_next_task| up_next_task["name"].as_str().expect("a name").to_owned())
        .collect()
}

fn find_task<'a>(up_next: &'a Value, task_name: &str) -> &'a Value {
    up_next["tasks"]
        .as_array()
        .expect("a list of tasks")
        .iter()
        .find(|up_next_task| up_next_task["name"] == task_name)
        .expect("the task should be listed")
}

async fn plan_block(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_id: &str,
    starts_at: &str,
    ends_at: &str,
) -> String {
    let response = post_accept(
        db_pool,
        signed_in_person,
        json!({ "blocks": [block(task_id, starts_at, ends_at)] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let accepted_plan = response_json(response).await;
    accepted_plan["events"][0]["id"]
        .as_str()
        .expect("an event id")
        .to_owned()
}

async fn move_block_start_back(db_pool: &PgPool, planned_event_id: &str, minute_count: i32) {
    sqlx::query(
        "UPDATE calendar_events SET starts_at = now() - make_interval(mins => $2)
         WHERE id = $1::uuid",
    )
    .bind(planned_event_id)
    .bind(minute_count)
    .execute(db_pool)
    .await
    .expect("moving the block back should run");
}

fn wall_clock_in(instant_value: &Value, time_zone_name: &str) -> (i8, i8) {
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
async fn lists_today_s_planned_tasks_first_then_the_rest_in_planning_order(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("Write", Some(30)), ("Review", None), ("Ship", Some(15))],
    )
    .await;
    let block_start = minutes_from_now(2);
    let planned_event_id = plan_block(
        &db_pool,
        &signed_in_person,
        &planned_group.task_ids[2],
        &block_start,
        &minutes_from_now(17),
    )
    .await;

    let up_next = get_up_next(&db_pool, &signed_in_person, None).await;

    assert_eq!(task_names(&up_next), vec!["Ship", "Write", "Review"]);
    let ship_task = find_task(&up_next, "Ship");
    assert_eq!(ship_task["plannedEvent"]["id"], planned_event_id.as_str());
    assert_eq!(ship_task["missed"], false);
    assert_eq!(ship_task["estimateMinutes"], 15);
    assert_eq!(ship_task["priority"], "normal");
    assert_eq!(ship_task["groupId"], planned_group.group_id.as_str());
    assert_eq!(find_task(&up_next, "Write")["plannedEvent"], Value::Null);
    assert_eq!(
        up_next["refreshAt"]
            .as_str()
            .expect("a refresh time")
            .parse::<Timestamp>()
            .expect("the refresh time should parse"),
        block_start
            .parse::<Timestamp>()
            .expect("the block start should parse")
    );
}

#[sqlx::test]
async fn marks_a_block_whose_start_passed_as_missed_until_the_task_is_started(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(&db_pool, &signed_in_person, &[("Write", Some(30))]).await;
    let write_task_id = &planned_group.task_ids[0];
    let planned_event_id = plan_block(
        &db_pool,
        &signed_in_person,
        write_task_id,
        &minutes_from_now(2),
        &minutes_from_now(32),
    )
    .await;
    move_block_start_back(&db_pool, &planned_event_id, 5).await;

    let missed_up_next = get_up_next(&db_pool, &signed_in_person, None).await;
    let running_item = start_task(&db_pool, &signed_in_person, write_task_id).await;
    let running_up_next = get_up_next(&db_pool, &signed_in_person, None).await;
    let finish_response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0) }),
    )
    .await;
    assert_eq!(finish_response.status(), StatusCode::OK);
    let finished_up_next = get_up_next(&db_pool, &signed_in_person, None).await;

    assert_eq!(find_task(&missed_up_next, "Write")["missed"], true);
    assert!(task_names(&running_up_next).is_empty());
    let started_task = find_task(&finished_up_next, "Write");
    assert_eq!(started_task["plannedEvent"], Value::Null);
    assert_eq!(started_task["missed"], false);
}

#[sqlx::test]
async fn leaves_out_done_tasks_and_groups_not_due_now(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let open_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("Write", None), ("Tidy", None)],
    )
    .await;
    set_task_done(&db_pool, &signed_in_person, &open_group.task_ids[1], true).await;
    let now = Timestamp::now().to_zoned(TimeZone::UTC);
    let window_start = format!("{:02}:00", (now.hour() + 2) % 24);
    let window_end = format!("{:02}:00", (now.hour() + 3) % 24);
    let later_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Later",
            "shows": {
                "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
                "windowStart": window_start,
                "windowEnd": window_end
            }
        }),
    )
    .await;
    add_task(
        &db_pool,
        &signed_in_person,
        later_group["id"].as_str().expect("a group id"),
        "Stretch",
        None,
    )
    .await;

    let up_next = get_up_next(&db_pool, &signed_in_person, None).await;

    assert_eq!(task_names(&up_next), vec!["Write"]);
    let next_change_hour = if now.hour() + 2 >= 24 {
        0
    } else {
        now.hour() + 2
    };
    assert_eq!(
        wall_clock_in(&up_next["refreshAt"], "UTC"),
        (next_change_hour, 0)
    );
}

#[sqlx::test]
async fn counts_a_tick_from_before_the_last_uncheck_as_open(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let daily_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Daily",
            "unchecks": {
                "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
                "time": "00:00"
            }
        }),
    )
    .await;
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        daily_group["id"].as_str().expect("a group id"),
        "Stretch",
        None,
    )
    .await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Stretch"),
        true,
    )
    .await;

    let fresh_up_next = get_up_next(&db_pool, &signed_in_person, None).await;
    sqlx::query("UPDATE tasks SET completed_at = now() - interval '2 days'")
        .execute(&db_pool)
        .await
        .expect("moving the tick back should run");
    let later_up_next = get_up_next(&db_pool, &signed_in_person, None).await;

    assert!(task_names(&fresh_up_next).is_empty());
    assert_eq!(task_names(&later_up_next), vec!["Stretch"]);
}

#[sqlx::test]
async fn refreshes_at_the_next_midnight_in_the_device_zone_without_a_stored_zone(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let up_next = get_up_next(&db_pool, &signed_in_person, Some("Asia/Manila")).await;

    assert_eq!(wall_clock_in(&up_next["refreshAt"], "Asia/Manila"), (0, 0));
}
