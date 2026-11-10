use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::{count_rows, seed_user};

const NEW_TABLES: [&str; 2] = ["card_triggers", "card_moves"];

struct SeededOwner {
    user_id: Uuid,
    board_id: Uuid,
    doing_list_id: Uuid,
    done_list_id: Uuid,
    card_id: Uuid,
}

async fn seed_list(
    db_pool: &PgPool,
    user_id: Uuid,
    board_id: Uuid,
    position: i32,
    list_name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO board_lists (id, user_id, board_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, $3, $4)
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(position)
    .bind(list_name)
    .fetch_one(db_pool)
    .await
    .expect("seed board list insert should succeed")
}

async fn seed_owner(db_pool: &PgPool, email: &str) -> SeededOwner {
    let user_id = seed_user(db_pool, email).await;
    let board_id: Uuid = sqlx::query_scalar(
        "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Projects')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed board insert should succeed");
    let doing_list_id = seed_list(db_pool, user_id, board_id, 0, "Doing").await;
    let done_list_id = seed_list(db_pool, user_id, board_id, 1, "Done").await;
    let card_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
         VALUES (gen_random_uuid(), $1, $2, $3, 0, 'Write the intro', '')
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(doing_list_id)
    .fetch_one(db_pool)
    .await
    .expect("seed card insert should succeed");
    SeededOwner {
        user_id,
        board_id,
        doing_list_id,
        done_list_id,
        card_id,
    }
}

async fn seed_trigger_and_move(db_pool: &PgPool, owner: &SeededOwner) {
    sqlx::query(
        "INSERT INTO card_triggers (card_id, kind, user_id, board_id, list_id)
         VALUES ($1, 'finished', $2, $3, $4)",
    )
    .bind(owner.card_id)
    .bind(owner.user_id)
    .bind(owner.board_id)
    .bind(owner.done_list_id)
    .execute(db_pool)
    .await
    .expect("seed trigger insert should succeed");
    sqlx::query(
        "INSERT INTO card_moves (id, user_id, card_id, moved_at, from_list_name, to_list_name, reason)
         VALUES (gen_random_uuid(), $1, $2, now(), 'To Do', 'Doing', 'by_hand')",
    )
    .bind(owner.user_id)
    .bind(owner.card_id)
    .execute(db_pool)
    .await
    .expect("seed card move insert should succeed");
}

async fn archive_from_done(db_pool: &PgPool, owner: &SeededOwner) {
    sqlx::query(
        "UPDATE cards
         SET list_id = NULL, position = NULL, archived_at = now(), archived_how = 'idle',
             archived_from_list_id = $2, archived_from_list_name = 'Done'
         WHERE id = $1",
    )
    .bind(owner.card_id)
    .bind(owner.done_list_id)
    .execute(db_pool)
    .await
    .expect("archiving the card should succeed");
}

async fn set_archive_after(
    db_pool: &PgPool,
    list_id: Uuid,
    archive_after: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE board_lists SET archive_after = $2::interval WHERE id = $1")
        .bind(list_id)
        .bind(archive_after)
        .execute(db_pool)
        .await
        .map(|_| ())
}

#[sqlx::test]
async fn deleting_a_user_removes_their_triggers_and_history(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;
    seed_trigger_and_move(&db_pool, &owner).await;
    seed_trigger_and_move(&db_pool, &other_owner).await;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner.user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    for new_table in NEW_TABLES {
        assert_eq!(count_rows(&db_pool, new_table).await, 1, "{new_table}");
    }
}

#[sqlx::test]
async fn deleting_a_card_removes_its_triggers_and_history(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    seed_trigger_and_move(&db_pool, &owner).await;

    sqlx::query("DELETE FROM cards WHERE id = $1")
        .bind(owner.card_id)
        .execute(&db_pool)
        .await
        .expect("card delete should succeed");

    for new_table in NEW_TABLES {
        assert_eq!(count_rows(&db_pool, new_table).await, 0, "{new_table}");
    }
}

#[sqlx::test]
async fn deleting_a_column_keeps_an_archived_card_that_left_it_and_flags_its_triggers(
    db_pool: PgPool,
) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    seed_trigger_and_move(&db_pool, &owner).await;
    archive_from_done(&db_pool, &owner).await;

    sqlx::query("DELETE FROM board_lists WHERE id = $1")
        .bind(owner.done_list_id)
        .execute(&db_pool)
        .await
        .expect("list delete should succeed");

    let (archived_from_list_id, archived_from_list_name): (Option<Uuid>, Option<String>) =
        sqlx::query_as(
            "SELECT archived_from_list_id, archived_from_list_name FROM cards WHERE id = $1",
        )
        .bind(owner.card_id)
        .fetch_one(&db_pool)
        .await
        .expect("the archived card should still exist");
    assert_eq!(archived_from_list_id, None);
    assert_eq!(archived_from_list_name.as_deref(), Some("Done"));
    let trigger_list_id: Option<Uuid> =
        sqlx::query_scalar("SELECT list_id FROM card_triggers WHERE card_id = $1")
            .bind(owner.card_id)
            .fetch_one(&db_pool)
            .await
            .expect("the trigger should stay as a flagged row");
    assert_eq!(trigger_list_id, None);
}

#[sqlx::test]
async fn deleting_a_column_still_deletes_the_cards_in_it(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    sqlx::query("DELETE FROM board_lists WHERE id = $1")
        .bind(owner.doing_list_id)
        .execute(&db_pool)
        .await
        .expect("list delete should succeed");

    assert_eq!(count_rows(&db_pool, "cards").await, 0);
}

#[sqlx::test]
async fn refuses_a_card_off_the_board_that_is_not_archived(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let update_result =
        sqlx::query("UPDATE cards SET list_id = NULL, position = NULL WHERE id = $1")
            .bind(owner.card_id)
            .execute(&db_pool)
            .await;

    assert!(update_result.is_err());
}

#[sqlx::test]
async fn refuses_an_archived_card_that_is_still_in_a_column(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let update_result =
        sqlx::query("UPDATE cards SET archived_at = now(), archived_how = 'by_hand' WHERE id = $1")
            .bind(owner.card_id)
            .execute(&db_pool)
            .await;

    assert!(update_result.is_err());
}

#[sqlx::test]
async fn accepts_archive_lengths_from_a_day_to_a_year(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    for archive_after in [
        "1 day",
        "2 weeks",
        "1 month 2 weeks",
        "12 months",
        "365 days",
    ] {
        assert!(
            set_archive_after(&db_pool, owner.done_list_id, archive_after)
                .await
                .is_ok(),
            "{archive_after}"
        );
    }
}

#[sqlx::test]
async fn refuses_archive_lengths_outside_a_day_to_a_year_or_with_hours(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    for archive_after in [
        "0 days",
        "23 hours",
        "1 day 2 hours",
        "13 months",
        "366 days",
    ] {
        assert!(
            set_archive_after(&db_pool, owner.done_list_id, archive_after)
                .await
                .is_err(),
            "{archive_after}"
        );
    }
}

#[sqlx::test]
async fn refuses_a_task_title_on_a_move_that_is_not_a_trigger(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let insert_result = sqlx::query(
        "INSERT INTO card_moves (id, user_id, card_id, moved_at, to_list_name, reason, task_title)
         VALUES (gen_random_uuid(), $1, $2, now(), 'Done', 'by_hand', 'Write the intro')",
    )
    .bind(owner.user_id)
    .bind(owner.card_id)
    .execute(&db_pool)
    .await;

    assert!(insert_result.is_err());
}

#[sqlx::test]
async fn refuses_a_trigger_pointing_at_another_boards_column(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;

    let insert_result = sqlx::query(
        "INSERT INTO card_triggers (card_id, kind, user_id, board_id, list_id)
         VALUES ($1, 'started', $2, $3, $4)",
    )
    .bind(owner.card_id)
    .bind(owner.user_id)
    .bind(owner.board_id)
    .bind(other_owner.doing_list_id)
    .execute(&db_pool)
    .await;

    assert!(insert_result.is_err());
}
