use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use daibx_api::features::cards::actions::archive_idle_cards::ArchiveIdleCards;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::activity::feed::{entry_kinds, list_entries};
use crate::board_lists::create::list_id;
use crate::calendar_events::create::event_id;
use crate::card_schedules::create::{
    board_with_review_card, create_schedule, schedule_id, weekly_review,
};
use crate::card_schedules::runs::run_now;
use crate::cards::archive::{archive_card, restore_card};
use crate::cards::create::{board_with_cards, card_id};
use crate::cards::history::drag_card;
use crate::cards::triggers::{import_linked_tasks, release_board, save_triggers};
use crate::planning::accept::{block, post_accept};
use crate::planning::proposal::{group_with_tasks, planning_person};
use crate::support::{
    build_test_app, patch_json_with_cookies, production_sized_pool, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::create::task_id_named;
use crate::tracking::finish::{minutes_from_now, post_finish};
use crate::tracking::start::start_task;

#[sqlx::test]
async fn dragging_a_card_to_another_column_adds_a_card_moved_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To Do", &["Plan"]), ("Doing", &[])],
    )
    .await;
    let plan_card_id = card_id(&board, "Plan").to_owned();

    drag_card(
        &db_pool,
        &signed_in_person,
        &plan_card_id,
        list_id(&board, "Doing"),
        0,
    )
    .await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["card_moved"]);
    assert_eq!(activity_entries[0]["title"], "Plan");
    assert_eq!(activity_entries[0]["cardId"], plan_card_id.as_str());
    assert_eq!(
        activity_entries[0]["details"],
        json!({ "fromListName": "To Do", "toListName": "Doing", "reason": "by_hand" })
    );
}

#[sqlx::test]
async fn reordering_inside_a_column_adds_a_by_hand_move(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board =
        board_with_cards(&db_pool, &signed_in_person, &[("To Do", &["Plan", "Ship"])]).await;

    drag_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Ship"),
        list_id(&board, "To Do"),
        0,
    )
    .await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["card_moved"]);
    assert_eq!(activity_entries[0]["details"]["fromListName"], "To Do");
    assert_eq!(activity_entries[0]["details"]["toListName"], "To Do");
}

#[sqlx::test]
async fn archiving_and_restoring_add_two_card_moved_entries(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Done", &["Release"])]).await;
    let release_card_id = card_id(&board, "Release").to_owned();

    archive_card(&db_pool, &signed_in_person, &release_card_id).await;
    restore_card(&db_pool, &signed_in_person, &release_card_id).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(
        entry_kinds(&activity_entries),
        vec!["card_moved", "card_moved"]
    );
    assert_eq!(
        activity_entries[0]["details"],
        json!({ "fromListName": null, "toListName": "Done", "reason": "restored" })
    );
    assert_eq!(
        activity_entries[1]["details"],
        json!({ "fromListName": "Done", "toListName": null, "reason": "archived_by_hand" })
    );
}

#[sqlx::test]
async fn an_idle_archive_adds_an_archived_idle_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Done", &["Release"])]).await;
    let release_card_id = card_id(&board, "Release").to_owned();
    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{}", list_id(&board, "Done")),
        json!({ "archiveAfter": "1m" }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    sqlx::query("UPDATE cards SET last_moved_at = '2030-01-10T08:00:00Z' WHERE id = $1")
        .bind(Uuid::parse_str(&release_card_id).expect("a card id"))
        .execute(&db_pool)
        .await
        .expect("the card's clock should be set");

    ArchiveIdleCards::new(&production_sized_pool(&db_pool))
        .execute(UtcInstant::parse("2030-02-10T08:00:00Z").expect("a valid test time"))
        .await
        .expect("the idle archive should run");

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["card_moved"]);
    assert_eq!(
        activity_entries[0]["occurredAt"],
        "2030-02-10T08:00:00.000Z"
    );
    assert_eq!(activity_entries[0]["details"]["reason"], "archived_idle");
}

#[sqlx::test]
async fn trigger_moves_add_their_own_entries_after_the_start_and_finish(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let release_board = release_board(&db_pool, &signed_in_person).await;
    save_triggers(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        json!({
            "startedListId": release_board.doing_list_id,
            "finishedListId": release_board.done_list_id,
        }),
    )
    .await;
    let group_tasks = import_linked_tasks(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        &["Ship"],
    )
    .await;
    let ship_task_id = task_id_named(&group_tasks, "Ship");

    let running_item = start_task(&db_pool, &signed_in_person, &ship_task_id).await;
    post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0), "markDone": true }),
    )
    .await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(
        entry_kinds(&activity_entries),
        vec!["finished", "card_moved", "card_moved", "started"]
    );
    assert_eq!(
        activity_entries[1]["details"],
        json!({ "fromListName": "Doing", "toListName": "Done", "reason": "finished", "taskTitle": "Ship" })
    );
    assert_eq!(
        activity_entries[2]["details"],
        json!({ "fromListName": "To Do", "toListName": "Doing", "reason": "started", "taskTitle": "Ship" })
    );
}

#[sqlx::test]
async fn a_schedule_run_that_moves_adds_one_schedule_ran_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    run_now(&db_pool, &signed_in_person, schedule_id(&weekly_schedule)).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["schedule_ran"]);
    assert_eq!(activity_entries[0]["title"], "Weekly review");
    assert_eq!(
        activity_entries[0]["cardId"],
        scheduled_card.card_id.as_str()
    );
    assert_eq!(
        activity_entries[0]["details"],
        json!({ "how": "by_hand", "moved": true, "fromListName": "Doing", "toListName": "To Do" })
    );
}

#[sqlx::test]
async fn a_schedule_run_that_finds_the_card_in_place_says_it_did_not_move(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.doing_list_id),
    )
    .await;

    run_now(&db_pool, &signed_in_person, schedule_id(&weekly_schedule)).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["schedule_ran"]);
    assert_eq!(
        activity_entries[0]["details"],
        json!({ "how": "by_hand", "moved": false, "fromListName": null, "toListName": null })
    );
}

#[sqlx::test]
async fn accepting_a_plan_adds_one_entry_with_the_block_count(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;
    let planned_group = group_with_tasks(
        &db_pool,
        &signed_in_person,
        &[("Write the plan", Some(30)), ("Review", Some(60))],
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
    response_json(response).await;
    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["plan_accepted"]);
    assert_eq!(activity_entries[0]["title"], serde_json::Value::Null);
    assert_eq!(activity_entries[0]["details"], json!({ "blockCount": 2 }));
}

#[sqlx::test]
async fn a_refused_plan_accept_adds_no_entry(db_pool: PgPool) {
    let signed_in_person = planning_person(&db_pool).await;

    let response = post_accept(
        &db_pool,
        &signed_in_person,
        json!({ "blocks": [
            block("00000000-0000-0000-0000-000000000000", "2030-01-07T09:00:00.000Z", "2030-01-07T09:30:00.000Z"),
        ] }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(list_entries(&db_pool, &signed_in_person).await.is_empty());
}
