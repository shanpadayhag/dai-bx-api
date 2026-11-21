use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::account::time_tracking::patch_time_tracking;
use crate::calendar_events::create::create_planned_event;
use crate::categories::update::find_category_id;
use crate::planning::proposal::{group_with_tasks, iso_after, planning_person, propose};
use crate::support::{SignedInPerson, build_test_app, post_json_with_cookies, response_json};
use crate::tasks::create::{add_task, task_id_named};

pub async fn post_accept(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: Value,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/plans/accept",
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub fn block(task_id: &str, starts_at: &str, ends_at: &str) -> Value {
    json!({ "taskId": task_id, "startsAt": starts_at, "endsAt": ends_at })
}

async fn count_events(db_pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM calendar_events")
        .fetch_one(db_pool)
        .await
        .expect("the count should run")
}

#[sqlx::test]
async fn writes_each_block_as_a_linked_categorized_and_zoned_planned_event(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    patch_time_tracking(
        &db_pool,
        &signed_in_person,
        json!({ "trackedCategoryId": studies_category_id, "calendarTimeZone": "Asia/Manila" }),
    )
    .await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("Write the plan", Some(30)), ("Review", Some(60))],
    )
    .await;
    create_planned_event(
        &db_pool,
        &signed_in_person,
        "Already there",
        "2030-01-07T09:00",
        "2030-01-07T10:00",
    )
    .await;

    let response = post_accept(
        &db_pool,
        &signed_in_person,
        json!({ "blocks": [
            block(&planned_group.task_ids[0], "2030-01-07T09:00:00.000Z", "2030-01-07T09:30:00.000Z"),
            block(&planned_group.task_ids[1], "2030-01-08T09:00:00.000Z", "2030-01-08T10:00:00.000Z"),
        ] }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let accepted_events = response_json(response).await["events"].clone();
    let accepted_events = accepted_events.as_array().expect("a list of events");
    assert_eq!(accepted_events.len(), 2);
    for (accepted_event, (task_id, task_name)) in accepted_events.iter().zip([
        (&planned_group.task_ids[0], "Write the plan"),
        (&planned_group.task_ids[1], "Review"),
    ]) {
        assert_eq!(accepted_event["kind"], "planned");
        assert_eq!(accepted_event["title"], task_name);
        assert_eq!(accepted_event["task"]["id"], task_id.as_str());
        assert_eq!(accepted_event["task"]["done"], false);
        assert_eq!(accepted_event["categoryId"], studies_category_id.as_str());
        assert_eq!(accepted_event["timeZone"], "Asia/Manila");
    }
    assert_eq!(accepted_events[0]["startsAt"], "2030-01-07T09:00:00.000Z");
    assert_eq!(count_events(&db_pool).await, 3);
}

#[sqlx::test]
async fn refuses_another_person_s_task_or_a_missing_one_and_writes_nothing(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let other_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(&db_pool, &signed_in_person, &[("Mine", Some(30))]).await;
    let other_group = group_with_tasks(&db_pool, &other_person, &[("Theirs", Some(30))]).await;

    for refused_task_id in [
        other_group.task_ids[0].clone(),
        uuid::Uuid::now_v7().to_string(),
        String::from("not-a-uuid"),
    ] {
        let response = post_accept(
            &db_pool,
            &signed_in_person,
            json!({ "blocks": [
                block(&planned_group.task_ids[0], "2030-01-07T09:00:00.000Z", "2030-01-07T09:30:00.000Z"),
                block(&refused_task_id, "2030-01-07T10:00:00.000Z", "2030-01-07T10:30:00.000Z"),
            ] }),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{refused_task_id}"
        );
    }
    assert_eq!(count_events(&db_pool).await, 0);
}

#[sqlx::test]
async fn refuses_a_subtask_a_bad_block_count_or_a_bad_block(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group =
        group_with_tasks(&db_pool, &signed_in_person, &[("Parent", Some(30))]).await;
    let parent_id = planned_group.task_ids[0].as_str();
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &planned_group.group_id,
        "Child",
        Some(parent_id),
    )
    .await;
    let subtask_id = task_id_named(&group_tasks, "Child");
    let good_block = block(
        parent_id,
        "2030-01-07T09:00:00.000Z",
        "2030-01-07T09:30:00.000Z",
    );

    for (refused_blocks, expected_error) in [
        (
            json!([block(
                &subtask_id,
                "2030-01-07T09:00:00.000Z",
                "2030-01-07T09:30:00.000Z"
            )]),
            "only a top-level task can be linked to a planned event",
        ),
        (json!([]), "accept 1 to 200 blocks"),
        (
            Value::Array(vec![good_block.clone(); 201]),
            "accept 1 to 200 blocks",
        ),
        (
            json!([block(
                parent_id,
                "2030-01-07T09:00:00.000Z",
                "2030-01-07T09:10:00.000Z"
            )]),
            "a block lasts at least 15 minutes",
        ),
        (
            json!([block(
                parent_id,
                "2030-01-07T09:00:00.000Z",
                "2030-01-07T08:00:00.000Z"
            )]),
            "a block ends after it starts",
        ),
    ] {
        let response = post_accept(
            &db_pool,
            &signed_in_person,
            json!({ "blocks": refused_blocks }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response_json(response).await["error"], expected_error);
    }
    assert_eq!(count_events(&db_pool).await, 0);
}

#[sqlx::test]
async fn an_accepted_task_is_reported_as_already_planned(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("Write the plan", Some(30)), ("Review", Some(30))],
    )
    .await;
    let first_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;
    let first_block = &first_proposal["blocks"][0];
    let starts_at = iso_after(first_block["startsAt"].as_str().expect("a start"), 0);

    let response = post_accept(
        &db_pool,
        &signed_in_person,
        json!({ "blocks": [block(
            first_block["taskId"].as_str().expect("a task id"),
            &starts_at,
            &iso_after(&starts_at, 30),
        )] }),
    )
    .await;
    let second_proposal = propose(&db_pool, &signed_in_person, json!({ "days": 7 })).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        second_proposal["blocks"][0]["taskId"],
        planned_group.task_ids[1].as_str()
    );
    assert_eq!(
        second_proposal["unplaced"],
        json!([{
            "taskId": planned_group.task_ids[0],
            "taskName": "Write the plan",
            "groupName": "Today",
            "reason": "alreadyPlanned",
        }])
    );
}
