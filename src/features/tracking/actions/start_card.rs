use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::open_running_event::OpenRunningEvent;
use crate::features::calendar_events::entities::calendar_event::{
    NewRunningEvent, RunningItem, TrackedSubject,
};
use crate::features::cards::actions::find_card_title::FindCardTitle;
use crate::features::cards::actions::fire_card_trigger::FireCardTrigger;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::entities::card_trigger::TriggerEvent;
use crate::features::tracking::error::TrackingError;

pub struct StartCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    find_card_title: FindCardTitle,
    open_running_event: OpenRunningEvent,
    fire_card_trigger: FireCardTrigger,
}

impl<'a> StartCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            find_card_title: FindCardTitle::new(),
            open_running_event: OpenRunningEvent::new(),
            fire_card_trigger: FireCardTrigger::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        now: UtcInstant,
    ) -> Result<RunningItem, TrackingError> {
        let mut start_transaction = self.db_pool.begin().await?;
        self.lock_board_of_card
            .execute(&mut start_transaction, user_id, card_id)
            .await?;
        let card_title = self
            .find_card_title
            .execute(&mut start_transaction, user_id, card_id)
            .await?;
        let running_item = self
            .open_running_event
            .execute(
                &mut start_transaction,
                &NewRunningEvent {
                    id: Uuid::now_v7(),
                    user_id,
                    title: card_title,
                    subject: TrackedSubject::Card(card_id),
                },
            )
            .await?;
        self.fire_card_trigger
            .execute(
                &mut start_transaction,
                user_id,
                card_id,
                TriggerEvent::Started,
                None,
                now,
            )
            .await?;
        start_transaction.commit().await?;
        Ok(running_item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::request_time_zone::RequestTimeZone;
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::cards::actions::create_card::CreateCard;
    use crate::features::tracking::actions::start_task::tests::seed_user_id;

    async fn seed_card_id(db_pool: &PgPool, user_id: Uuid) -> Uuid {
        let board = CreateBoard::new()
            .execute(db_pool, user_id, "Projects")
            .await
            .expect("the board should be created");
        let board = CreateBoardList::new(db_pool)
            .execute(
                user_id,
                board.board.id,
                "Doing",
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the list should be created");
        let list_id = board.lists.first().expect("the list should be listed").id;
        let board = CreateCard::new(db_pool)
            .execute(
                user_id,
                list_id,
                "Plan",
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the card should be created");
        board.cards.first().expect("the card should be listed").id
    }

    #[sqlx::test]
    async fn starts_the_owner_s_card_once(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let card_id = seed_card_id(&db_pool, owner_id).await;
        let start_card = StartCard::new(&db_pool);

        let running_item = start_card
            .execute(owner_id, card_id, UtcInstant::now())
            .await
            .expect("the card should start");
        let second_start = start_card
            .execute(owner_id, card_id, UtcInstant::now())
            .await;
        let foreign_start = start_card
            .execute(other_person_id, card_id, UtcInstant::now())
            .await;

        assert_eq!(running_item.event.title, "Plan");
        assert_eq!(running_item.estimate_minutes, None);
        assert!(matches!(second_start, Err(TrackingError::AlreadyRunning)));
        assert!(matches!(foreign_start, Err(TrackingError::NotFound)));
    }
}
