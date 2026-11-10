use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use daibx_api::features::cards::actions::archive_idle_cards::ArchiveIdleCards;
use daibx_api::features::cards::entities::card_archive::IdleArchiveTally;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::board_lists::create::list_id;
use crate::card_schedules::create::read_card;
use crate::cards::create::{board_with_cards, card_id};
use crate::cards::history::{move_shapes, read_history};
use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, sign_up_with_authenticator,
};
use crate::tracking::start::start_card;

const LAST_MOVED_AT: &str = "2030-01-10T08:00:00Z";

async fn call_scheduler(db_pool: &PgPool, now_text: &str) -> IdleArchiveTally {
    ArchiveIdleCards::new(db_pool)
        .execute(UtcInstant::parse(now_text).expect("a valid test time"))
        .await
        .expect("the idle archive call should run")
}

fn archived(archived_count: usize) -> IdleArchiveTally {
    IdleArchiveTally {
        archived_count,
        failed_count: 0,
    }
}

async fn set_archive_after(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    list_id: &str,
    archive_after: &str,
) {
    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{list_id}"),
        json!({ "archiveAfter": archive_after }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn set_last_moved_at(db_pool: &PgPool, card_id: &str) {
    sqlx::query("UPDATE cards SET last_moved_at = $1::timestamptz WHERE id = $2")
        .bind(LAST_MOVED_AT)
        .bind(Uuid::parse_str(card_id).expect("a card id"))
        .execute(db_pool)
        .await
        .expect("the card's clock should be set");
}

async fn is_archived(db_pool: &PgPool, signed_in_person: &SignedInPerson, card_id: &str) -> bool {
    read_card(db_pool, signed_in_person, card_id).await["archived"].is_object()
}

#[sqlx::test]
async fn archives_a_card_a_calendar_month_after_it_last_moved(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Done", &["Release"])]).await;
    let release_card_id = card_id(&board, "Release").to_owned();
    set_archive_after(&db_pool, &signed_in_person, list_id(&board, "Done"), "1m").await;
    set_last_moved_at(&db_pool, &release_card_id).await;

    let early_tally = call_scheduler(&db_pool, "2030-02-10T07:59:00Z").await;
    let due_tally = call_scheduler(&db_pool, "2030-02-10T08:00:00Z").await;
    let later_tally = call_scheduler(&db_pool, "2030-02-10T08:01:00Z").await;

    assert_eq!(early_tally, archived(0));
    assert_eq!(due_tally, archived(1));
    assert_eq!(later_tally, archived(0));
    let archived_detail = read_card(&db_pool, &signed_in_person, &release_card_id).await;
    assert_eq!(
        archived_detail["archived"],
        json!({ "at": "2030-02-10T08:00:00.000Z", "how": "idle" })
    );
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_card_id).await),
        vec![json!({ "from": "Done", "to": null, "reason": "archivedIdle", "taskTitle": null })]
    );
}

#[sqlx::test]
async fn skips_a_running_card_and_a_column_without_a_length(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Done", &["Running"]), ("Backlog", &["Waiting"])],
    )
    .await;
    set_archive_after(&db_pool, &signed_in_person, list_id(&board, "Done"), "1w").await;
    for card_title in ["Running", "Waiting"] {
        set_last_moved_at(&db_pool, card_id(&board, card_title)).await;
    }
    start_card(&db_pool, &signed_in_person, card_id(&board, "Running")).await;

    let tally = call_scheduler(&db_pool, "2030-06-01T00:00:00Z").await;

    assert_eq!(tally, archived(0));
    for card_title in ["Running", "Waiting"] {
        assert!(!is_archived(&db_pool, &signed_in_person, card_id(&board, card_title)).await);
    }
}

#[sqlx::test]
async fn a_shortened_length_archives_cards_past_it_on_the_next_call(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Done", &["Release"])]).await;
    let release_card_id = card_id(&board, "Release").to_owned();
    let done_list_id = list_id(&board, "Done").to_owned();
    set_archive_after(&db_pool, &signed_in_person, &done_list_id, "1m").await;
    set_last_moved_at(&db_pool, &release_card_id).await;

    let before_change = call_scheduler(&db_pool, "2030-01-20T08:00:00Z").await;
    set_archive_after(&db_pool, &signed_in_person, &done_list_id, "1w").await;
    let after_change = call_scheduler(&db_pool, "2030-01-20T08:01:00Z").await;

    assert_eq!(before_change, archived(0));
    assert_eq!(after_change, archived(1));
    assert!(is_archived(&db_pool, &signed_in_person, &release_card_id).await);
}
