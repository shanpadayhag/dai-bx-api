use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::list_calendar_events::ListCalendarEvents;
use crate::features::calendar_events::entities::calendar_event::EventRange;
use crate::features::planning::data_sources::postgres::planning_source::PostgresPlanningSource;
use crate::features::planning::entities::up_next::{UpNext, UpNextInputs, local_day_span};
use crate::features::planning::error::PlanningError;
use crate::features::task_groups::actions::find_group_rules::FindGroupRules;
use crate::features::task_groups::entities::group_timetable::GroupTimetable;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

pub struct FindUpNext<'a> {
    db_pool: &'a PgPool,
    find_group_rules: FindGroupRules,
    postgres_planning_source: PostgresPlanningSource,
}

impl<'a> FindUpNext<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_group_rules: FindGroupRules::new(),
            postgres_planning_source: PostgresPlanningSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<UpNext, PlanningError> {
        let (group_rules_listing, planning_tasks) = tokio::try_join!(
            async {
                Ok::<_, PlanningError>(self.find_group_rules.execute(self.db_pool, user_id).await?)
            },
            async {
                Ok::<_, PlanningError>(
                    self.postgres_planning_source
                        .list_planning_tasks(self.db_pool, user_id, None, now)
                        .await?,
                )
            },
        )?;
        let time_zone = request_time_zone
            .calendar_time_zone(group_rules_listing.calendar_time_zone_name.as_deref());
        let (day_start, day_end) = local_day_span(now, &time_zone)
            .ok_or_else(|| PlanningError::Invalid(String::from("today is out of bounds")))?;
        let calendar_range = ListCalendarEvents::new(self.db_pool)
            .execute(
                user_id,
                EventRange {
                    from: day_start,
                    to: day_end,
                },
            )
            .await?;
        let group_timetables: HashMap<Uuid, GroupTimetable> = group_rules_listing
            .work_out_timetables(request_time_zone, now)
            .into_iter()
            .map(|(task_group_id, _, group_timetable)| (task_group_id, group_timetable))
            .collect();
        let uncheck_cutoffs = UncheckCutoffs::from_timetables(
            group_timetables
                .iter()
                .map(|(task_group_id, group_timetable)| (*task_group_id, group_timetable)),
        );
        Ok(UpNext::work_out(&UpNextInputs {
            planning_tasks: &planning_tasks,
            group_timetables: &group_timetables,
            uncheck_cutoffs: &uncheck_cutoffs,
            today_events: &calendar_range.events,
            time_zone: &time_zone,
            now,
            day_end,
        }))
    }
}
