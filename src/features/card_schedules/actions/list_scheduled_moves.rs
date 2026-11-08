use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::ScheduledMove;
use crate::features::card_schedules::error::ScheduleError;

pub struct ListScheduledMoves<'a> {
    db_pool: &'a PgPool,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl<'a> ListScheduledMoves<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        from: UtcInstant,
        to: UtcInstant,
    ) -> Result<Vec<ScheduledMove>, ScheduleError> {
        let mut scheduled_moves: Vec<ScheduledMove> = self
            .postgres_card_schedule_source
            .list_active(self.db_pool, user_id)
            .await?
            .iter()
            .flat_map(|active_schedule| active_schedule.moves_in(from, to))
            .collect();
        scheduled_moves
            .sort_by_key(|scheduled_move| (scheduled_move.at, scheduled_move.schedule_id));
        Ok(scheduled_moves)
    }
}
