use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::actions::move_card_to_list_top::MoveCardToListTop;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::data_sources::postgres::card_trigger_source::PostgresCardTriggerSource;
use crate::features::cards::entities::card_move::{CardMoveReason, MoveCause};
use crate::features::cards::entities::card_trigger::{TriggerEvent, TriggerKind};
use crate::features::cards::error::CardError;

#[derive(Default)]
pub struct FireCardTrigger {
    lock_board_of_card: LockBoardOfCard,
    move_card_to_list_top: MoveCardToListTop,
    postgres_card_source: PostgresCardSource,
    postgres_card_trigger_source: PostgresCardTriggerSource,
}

impl FireCardTrigger {
    pub fn new() -> Self {
        Self {
            lock_board_of_card: LockBoardOfCard::new(),
            move_card_to_list_top: MoveCardToListTop::new(),
            postgres_card_source: PostgresCardSource,
            postgres_card_trigger_source: PostgresCardTriggerSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        card_id: Uuid,
        trigger_event: TriggerEvent<'_>,
        task_title: Option<&str>,
        now: UtcInstant,
    ) -> Result<bool, CardError> {
        let board_summary = self
            .lock_board_of_card
            .execute(&mut *connection, user_id, card_id)
            .await?;
        if let TriggerEvent::Finished { uncheck_cutoffs } = trigger_event
            && self
                .postgres_card_source
                .has_open_linked_tasks(&mut *connection, card_id, uncheck_cutoffs)
                .await?
        {
            return Ok(false);
        }
        let trigger_kind = trigger_event.kind();
        let Some(target_list_id) = self
            .postgres_card_trigger_source
            .find_target_list_id(&mut *connection, card_id, trigger_kind)
            .await?
        else {
            return Ok(false);
        };
        let move_cause = MoveCause {
            reason: match trigger_kind {
                TriggerKind::Started => CardMoveReason::Started,
                TriggerKind::Finished => CardMoveReason::Finished,
            },
            task_title: task_title.map(str::to_owned),
            moved_at: now,
        };
        self.move_card_to_list_top
            .execute(
                &mut *connection,
                board_summary.id,
                card_id,
                target_list_id,
                &move_cause,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::core::request_time_zone::RequestTimeZone;
    use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;
    use sqlx::PgPool;

    use super::*;
    use crate::features::cards::actions::archive_card::ArchiveCard;
    use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
    use crate::features::cards::data_sources::postgres::card_source::tests::{
        seed_board_with_lists, seed_card, seed_user_id,
    };
    use crate::features::cards::entities::card_trigger::TriggerChoice;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;

    async fn fire(db_pool: &PgPool, user_id: Uuid, card_id: Uuid, kind: TriggerKind) -> bool {
        let uncheck_cutoffs = UncheckCutoffs::default();
        let trigger_event = match kind {
            TriggerKind::Started => TriggerEvent::Started,
            TriggerKind::Finished => TriggerEvent::Finished {
                uncheck_cutoffs: &uncheck_cutoffs,
            },
        };
        let mut fire_transaction = db_pool.begin().await.expect("a transaction");
        let has_moved = FireCardTrigger::new()
            .execute(
                &mut fire_transaction,
                user_id,
                card_id,
                trigger_event,
                Some("Ship"),
                UtcInstant::now(),
            )
            .await
            .expect("the trigger should run");
        fire_transaction.commit().await.expect("the commit");
        has_moved
    }

    async fn move_count(db_pool: &PgPool, card_id: Uuid) -> usize {
        PostgresCardMoveSource
            .list_for_card(db_pool, card_id, None, 50)
            .await
            .expect("the history read should run")
            .len()
    }

    #[sqlx::test]
    async fn fires_started_on_every_start_but_not_when_already_there_or_archived(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, user_id, &["To Do", "Doing"]).await;
        let (to_do_list_id, doing_list_id) = (board.lists[0].id, board.lists[1].id);
        let card_id = seed_card(&db_pool, user_id, board.board.id, to_do_list_id, "Plan").await;
        PostgresCardTriggerSource
            .replace(
                &db_pool,
                board.board.id,
                card_id,
                TriggerChoice {
                    started_list_id: Some(doing_list_id),
                    finished_list_id: None,
                },
            )
            .await
            .expect("the triggers should save");

        let first_fire = fire(&db_pool, user_id, card_id, TriggerKind::Started).await;
        let repeat_fire = fire(&db_pool, user_id, card_id, TriggerKind::Started).await;
        let finished_fire = fire(&db_pool, user_id, card_id, TriggerKind::Finished).await;
        ArchiveCard::new(&db_pool)
            .execute(user_id, card_id, UtcInstant::now())
            .await
            .expect("the card should archive");
        let archived_fire = fire(&db_pool, user_id, card_id, TriggerKind::Started).await;

        assert!(first_fire);
        assert!(!repeat_fire);
        assert!(!finished_fire);
        assert!(!archived_fire);
        assert_eq!(move_count(&db_pool, card_id).await, 2);
    }

    #[sqlx::test]
    async fn finished_waits_for_the_last_open_linked_task(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, user_id, &["To Do", "Done"]).await;
        let card_id = seed_card(&db_pool, user_id, board.board.id, board.lists[0].id, "Plan").await;
        PostgresCardTriggerSource
            .replace(
                &db_pool,
                board.board.id,
                card_id,
                TriggerChoice {
                    started_list_id: None,
                    finished_list_id: Some(board.lists[1].id),
                },
            )
            .await
            .expect("the triggers should save");
        let task_group = CreateTaskGroup::new()
            .execute(&db_pool, user_id, "Today")
            .await
            .expect("the group should be created");
        for task_name in ["Draft", "Send"] {
            AddTask::new(&db_pool)
                .execute(
                    user_id,
                    task_group.id,
                    task_name,
                    None,
                    &RequestTimeZone::default(),
                    UtcInstant::now(),
                )
                .await
                .expect("the task should be added");
        }
        let linked_task_ids: Vec<Uuid> =
            sqlx::query_scalar("UPDATE tasks SET card_id = $1 WHERE group_id = $2 RETURNING id")
                .bind(card_id)
                .bind(task_group.id)
                .fetch_all(&db_pool)
                .await
                .expect("tasks should link");
        let mut fires = Vec::new();
        for linked_task_id in linked_task_ids {
            fires.push(fire(&db_pool, user_id, card_id, TriggerKind::Finished).await);
            sqlx::query("UPDATE tasks SET completed_at = now() WHERE id = $1")
                .bind(linked_task_id)
                .execute(&db_pool)
                .await
                .expect("the task should be marked done");
        }
        fires.push(fire(&db_pool, user_id, card_id, TriggerKind::Finished).await);

        assert_eq!(fires, vec![false, false, true]);
    }
}
