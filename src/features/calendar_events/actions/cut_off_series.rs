use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::event_series::EventSeries;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct CutOffSeries {
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

impl CutOffSeries {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
            postgres_event_series_source: PostgresEventSeriesSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        series: &EventSeries,
        original_start: UtcInstant,
    ) -> Result<(), CalendarEventError> {
        self.postgres_event_series_source
            .update(
                &mut *connection,
                user_id,
                &EventSeries {
                    until_instant: Some(original_start),
                    ..series.clone()
                },
            )
            .await?;
        let later_override_starts: Vec<UtcInstant> = self
            .postgres_calendar_event_source
            .list_override_starts(&mut *connection, series.id)
            .await?
            .into_iter()
            .filter(|override_start| *override_start >= original_start)
            .collect();
        let later_cancelled_starts: Vec<UtcInstant> = series
            .cancelled_starts
            .iter()
            .copied()
            .filter(|cancelled_start| *cancelled_start >= original_start)
            .collect();
        self.postgres_calendar_event_source
            .delete_overrides(&mut *connection, series.id, &later_override_starts)
            .await?;
        self.postgres_event_series_source
            .delete_exceptions(&mut *connection, series.id, &later_cancelled_starts)
            .await?;
        Ok(())
    }
}
