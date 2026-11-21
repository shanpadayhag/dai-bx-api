use sqlx::PgConnection;

use crate::features::activity::actions::record_activity::RecordActivity;
use crate::features::activity::entities::activity_entry::{
    ActivityDetails, ActivitySubject, NewActivityEntry,
};
use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
use crate::features::cards::entities::card_move::{CardMoveReason, NewCardMove, RecordedCardMove};
use crate::features::cards::error::CardError;

#[derive(Default)]
pub struct RecordCardMove {
    postgres_card_move_source: PostgresCardMoveSource,
    record_activity: RecordActivity,
}

impl RecordCardMove {
    pub fn new() -> Self {
        Self {
            postgres_card_move_source: PostgresCardMoveSource,
            record_activity: RecordActivity::new(),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        new_card_move: &NewCardMove<'_>,
    ) -> Result<Option<RecordedCardMove>, CardError> {
        let recorded_card_move = self
            .postgres_card_move_source
            .record(&mut *connection, new_card_move)
            .await?;
        let move_cause = new_card_move.cause;
        if let Some(recorded_card_move) = &recorded_card_move
            && move_cause.reason != CardMoveReason::Schedule
        {
            self.record_activity
                .execute(
                    &mut *connection,
                    NewActivityEntry {
                        user_id: recorded_card_move.user_id,
                        occurred_at: move_cause.moved_at,
                        subject: ActivitySubject::Card(new_card_move.card_id),
                        details: ActivityDetails::CardMoved {
                            from_list_name: recorded_card_move.from_list_name.clone(),
                            to_list_name: recorded_card_move.to_list_name.clone(),
                            reason: move_cause.reason.as_str().to_owned(),
                            task_title: move_cause.task_title.clone(),
                        },
                    },
                )
                .await?;
        }
        Ok(recorded_card_move)
    }
}
