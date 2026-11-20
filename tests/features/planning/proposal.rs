use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::account::time_tracking::patch_time_tracking;
use crate::calendar_events::create::post_event;
use crate::calendar_events::list::list_events;
use crate::card_schedules::create::{board_with_review_card, create_schedule, merged, once_at};
use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, create_group_id, task_id_named};
use crate::tracking::start::start_task;

pub async fn post_proposal(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: Value,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/plans/proposal",
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn propose(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: Value,
) -> Value {
    let response = post_proposal(db_pool, signed_in_person, request_body).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub async fn planning_person(db_pool: &PgPool) -> SignedInPerson {
    let signed_in_person = sign_up_with_authenticator(db_pool).await;
    let whole_day = json!({ "start": "00:00", "end": "23:59" });
    let response = patch_time_tracking(
        db_pool,
        &signed_in_person,
        json!({
            "calendarTimeZone": "UTC",
            "planningHours": Value::Array(vec![whole_day; 7]),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    signed_in_person
}

pub async fn set_estimate(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_id: &str,
    estimate_minutes: Option<i64>,
) {
    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tasks/{task_id}"),
        json!({ "estimateMinutes": estimate_minutes }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

pub struct PlannedGroup {
    pub group_id: String,
    pub task_ids: Vec<String>,
}

pub async fn group_with_tasks(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    estimated_tasks: &[(&str, Option<i64>)],
) -> PlannedGroup {
    let group_id = create_group_id(db_pool, signed_in_person).await;
    let mut task_ids = Vec::new();
    for (task_name, estimate_minutes) in estimated_tasks {
        let group_tasks = add_task(db_pool, signed_in_person, &group_id, task_name, None).await;
        let task_id = task_id_named(&group_tasks, task_name);
        set_estimate(db_pool, signed_in_person, &task_id, *estimate_minutes).await;
        task_ids.push(task_id);
    }
    PlannedGroup { group_id, task_ids }
}

pub fn millis_of(instant_text: &str) -> i64 {
    instant_text
        .parse::<jiff::Timestamp>()
        .expect("an ISO instant")
        .as_millisecond()
}

pub fn iso_after(instant_text: &str, minute_count: i64) -> String {
    let shifted = instant_text
        .parse::<jiff::Timestamp>()
        .expect("an ISO instant")
        .checked_add(jiff::SignedDuration::from_mins(minute_count))
        .expect("a shifted instant");
    shifted.strftime("%Y-%m-%dT%H:%M:%S.000Z").to_string()
}

fn local_minute(instant_text: &str) -> &str {
    &instant_text[..16]
}

fn block_span(block_json: &Value) -> (i64, i64) {
    (
        millis_of(block_json["startsAt"].as_str().expect("a start")),
        millis_of(block_json["endsAt"].as_str().expect("an end")),
    )
}

fn block_task_ids(proposal: &Value) -> Vec<&str> {
    proposal["blocks"]
        .as_array()
        .expect("a list of blocks")
        .iter()
        .map(|block_json| block_json["taskId"].as_str().expect("a task id"))
        .collect()
}

fn unplaced_reasons(proposal: &Value) -> Vec<(&str, &str)> {
    proposal["unplaced"]
        .as_array()
        .expect("a list of unplaced tasks")
        .iter()
        .map(|unplaced_json| {
            (
                unplaced_json["taskId"].as_str().expect("a task id"),
                unplaced_json["reason"].as_str().expect("a reason"),
            )
        })
        .collect()
}

fn assert_no_overlap(proposal: &Value, busy_spans: &[(i64, i64)]) {
    let blocks = proposal["blocks"].as_array().expect("a list of blocks");
    for (block_index, block_json) in blocks.iter().enumerate() {
        let (block_start, block_end) = block_span(block_json);
        for (busy_start, busy_end) in busy_spans {
            assert!(
                block_end <= *busy_start || block_start >= *busy_end,
                "{block_json} overlaps {busy_start}..{busy_end}"
            );
        }
        for other_block in &blocks[block_index + 1..] {
            let (other_start, other_end) = block_span(other_block);
            assert!(block_end <= other_start || block_start >= other_end);
        }
    }
}

#[sqlx::test]
async fn proposes_quarter_hour_blocks_for_one_and_seven_days(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("Write the plan", Some(30)), ("Review the notes", Some(40))],
    )
    .await;

    let week_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;
    let day_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 1 })).await;

    let plan_start = millis_of(week_proposal["planStart"].as_str().expect("a plan start"));
    assert_eq!(plan_start % (15 * 60_000), 0);
    assert!(millis_of(day_proposal["rangeEnd"].as_str().expect("a range end")) >= plan_start);
    let blocks = week_proposal["blocks"]
        .as_array()
        .expect("a list of blocks");
    assert_eq!(block_task_ids(&week_proposal), planned_group.task_ids);
    for (block_json, expected_minutes) in blocks.iter().zip([30, 45]) {
        let (block_start, block_end) = block_span(block_json);
        assert!(block_start >= plan_start);
        assert_eq!(block_start % (15 * 60_000), 0);
        assert_eq!(block_end - block_start, expected_minutes * 60_000);
        assert_eq!(block_json["groupId"], planned_group.group_id.as_str());
        assert_eq!(block_json["groupName"], "Today");
        assert_eq!(block_json["late"], false);
        assert_eq!(block_json["due"], Value::Null);
    }
    assert_no_overlap(&week_proposal, &[]);
}

#[sqlx::test]
async fn reports_why_a_task_was_left_unplaced(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[
            ("No estimate", None),
            ("Running", Some(30)),
            ("Already planned", Some(30)),
            ("Too long", Some(1440)),
            ("Done", Some(30)),
        ],
    )
    .await;
    let [no_estimate_id, running_id, planned_id, too_long_id, done_id] =
        planned_group.task_ids.as_slice()
    else {
        panic!("five tasks");
    };
    start_task(&db_pool, &signed_in_person, running_id).await;
    set_task_done(&db_pool, &signed_in_person, done_id, true).await;
    let first_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;
    let block_start = iso_after(
        first_proposal["planStart"].as_str().expect("a start"),
        24 * 60,
    );
    let planned_response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Already planned",
            "start": local_minute(&block_start),
            "end": local_minute(&iso_after(&block_start, 30)),
            "inputZone": "UTC",
            "taskId": planned_id,
        }),
    )
    .await;
    assert_eq!(planned_response.status(), StatusCode::CREATED);

    let proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;

    assert_eq!(block_task_ids(&proposal), Vec::<&str>::new());
    assert_eq!(
        unplaced_reasons(&proposal),
        vec![
            (no_estimate_id.as_str(), "noEstimate"),
            (running_id.as_str(), "running"),
            (planned_id.as_str(), "alreadyPlanned"),
            (too_long_id.as_str(), "doesNotFit"),
        ]
    );
}

#[sqlx::test]
async fn filters_groups_and_leaves_out_excluded_and_pinned_tasks(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let first_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[
            ("Excluded", Some(30)),
            ("Pinned", Some(60)),
            ("Planned", Some(30)),
        ],
    )
    .await;
    let second_group =
        group_with_tasks(&db_pool, &signed_in_person, &[("Elsewhere", Some(30))]).await;
    let first_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;
    let plan_start = first_proposal["planStart"]
        .as_str()
        .expect("a start")
        .to_owned();
    let pinned_end = iso_after(&plan_start, 60);

    let proposal = propose(
        &db_pool,
        &signed_in_person,
        json!({
            "days": 7,
            "groupIds": [first_group.group_id],
            "excludeTaskIds": [first_group.task_ids[0]],
            "pinned": [{ "taskId": first_group.task_ids[1], "startsAt": plan_start, "endsAt": pinned_end }],
        }),
    )
    .await;

    assert_eq!(block_task_ids(&first_proposal).len(), 4);
    assert_eq!(
        block_task_ids(&proposal),
        vec![first_group.task_ids[2].as_str()]
    );
    assert_eq!(proposal["unplaced"], json!([]));
    assert_eq!(proposal["blocks"][0]["startsAt"], pinned_end.as_str());
    assert!(!block_task_ids(&proposal).contains(&second_group.task_ids[0].as_str()));
}

#[sqlx::test]
async fn refuses_a_bad_proposal_request(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let other_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(&db_pool, &signed_in_person, &[("Write", Some(30))]).await;
    let other_group = group_with_tasks(&db_pool, &other_person, &[("Theirs", Some(30))]).await;
    let task_id = planned_group.task_ids[0].as_str();
    let pinned_block = json!({
        "taskId": task_id,
        "startsAt": "2030-01-07T09:00:00.000Z",
        "endsAt": "2030-01-07T10:00:00.000Z",
    });

    for (refused_body, expected_error) in [
        (json!({ "days": 3 }), "days is 1 or 7"),
        (
            json!({ "days": 7, "groupIds": [uuid::Uuid::now_v7().to_string()] }),
            "a group in groupIds isn't one of yours",
        ),
        (
            json!({ "days": 7, "groupIds": [other_group.group_id] }),
            "a group in groupIds isn't one of yours",
        ),
        (
            json!({ "days": 7, "pinned": Value::Array(vec![pinned_block.clone(); 201]) }),
            "a plan keeps at most 200 pinned blocks",
        ),
        (
            json!({ "days": 7, "pinned": [{
                "taskId": task_id,
                "startsAt": "2030-01-07T09:00:00.000Z",
                "endsAt": "2030-01-07T09:10:00.000Z",
            }] }),
            "a block lasts at least 15 minutes",
        ),
    ] {
        let response = post_proposal(&db_pool, &signed_in_person, refused_body.clone()).await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_body}"
        );
        assert_eq!(response_json(response).await["error"], expected_error);
    }
}

#[sqlx::test]
async fn never_places_a_block_over_any_kind_of_event(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[
            ("One", Some(60)),
            ("Two", Some(60)),
            ("Three", Some(60)),
            ("Four", Some(60)),
            ("Five", Some(60)),
            ("Six", Some(60)),
        ],
    )
    .await;
    let tracked_group = group_with_tasks(&db_pool, &signed_in_person, &[("Tracked", None)]).await;
    start_task(&db_pool, &signed_in_person, &tracked_group.task_ids[0]).await;
    let plan_start = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await["planStart"]
        .as_str()
        .expect("a start")
        .to_owned();
    for (title, start_minutes, end_minutes, repeat) in [
        ("Planned", 0, 45, Value::Null),
        (
            "Series",
            90,
            150,
            json!({ "frequency": "daily", "interval": 1, "ends": { "never": true } }),
        ),
    ] {
        let mut event_body = json!({
            "title": title,
            "start": local_minute(&iso_after(&plan_start, start_minutes)),
            "end": local_minute(&iso_after(&plan_start, end_minutes)),
            "inputZone": "UTC",
        });
        if !repeat.is_null() {
            event_body["repeat"] = repeat;
        }
        let response = post_event(&db_pool, &signed_in_person, event_body).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{title}");
    }
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            once_at(
                &scheduled_card.to_do_list_id,
                local_minute(&iso_after(&plan_start, 180)),
            ),
            json!({ "block": { "minutes": 60 } }),
        ),
    )
    .await;

    let proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;

    let today = &plan_start[..10];
    let listed_events = list_events(
        &db_pool,
        &signed_in_person,
        &format!("start={today}&days=7&timeZone=UTC"),
    )
    .await;
    let busy_spans: Vec<(i64, i64)> = listed_events
        .iter()
        .map(|event_json| {
            let event_start = millis_of(event_json["startsAt"].as_str().expect("a start"));
            let event_end = event_json["endsAt"]
                .as_str()
                .map_or(millis_of(&plan_start), millis_of);
            (event_start, event_end)
        })
        .collect();
    let event_kinds: Vec<(&Value, &Value, &Value)> = listed_events
        .iter()
        .map(|event_json| {
            (
                &event_json["kind"],
                &event_json["series"],
                &event_json["isScheduleBlock"],
            )
        })
        .collect();
    assert!(
        event_kinds
            .iter()
            .any(|(_, _, is_block)| **is_block == true)
    );
    assert!(event_kinds.iter().any(|(_, series, _)| !series.is_null()));
    assert!(event_kinds.iter().any(|(kind, _, _)| **kind == "tracked"));
    assert_eq!(
        block_task_ids(&proposal).len(),
        planned_group.task_ids.len()
    );
    assert_no_overlap(&proposal, &busy_spans);
}

#[sqlx::test]
async fn proposing_twice_gives_the_same_plan_and_writes_nothing(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("One", Some(30)), ("Two", Some(90)), ("Three", None)],
    )
    .await;
    let count_events = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM calendar_events")
            .fetch_one(&db_pool)
            .await
            .expect("the count should run")
    };
    let events_before = count_events().await;

    let first_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;
    let second_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;

    if first_proposal["planStart"] == second_proposal["planStart"] {
        assert_eq!(first_proposal, second_proposal);
    }
    assert_eq!(count_events().await, events_before);
}
