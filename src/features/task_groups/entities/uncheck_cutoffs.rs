use uuid::Uuid;

use crate::features::task_groups::entities::group_timetable::GroupTimetable;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UncheckCutoffs {
    pub group_ids: Vec<Uuid>,
    pub cutoff_millis: Vec<i64>,
}

impl UncheckCutoffs {
    pub fn cutoff_millis_for(&self, task_group_id: Uuid) -> Option<i64> {
        self.group_ids
            .iter()
            .position(|group_id| *group_id == task_group_id)
            .and_then(|group_index| self.cutoff_millis.get(group_index).copied())
    }

    pub fn from_timetables<'a>(
        group_timetables: impl Iterator<Item = (Uuid, &'a GroupTimetable)>,
    ) -> Self {
        let (group_ids, cutoff_millis) = group_timetables
            .filter_map(|(group_id, group_timetable)| {
                group_timetable
                    .last_uncheck_at
                    .map(|last_uncheck_at| (group_id, last_uncheck_at.unix_millis()))
            })
            .unzip();
        Self {
            group_ids,
            cutoff_millis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::utc_instant::UtcInstant;

    fn timetable_with_last_uncheck(last_uncheck_at: Option<UtcInstant>) -> GroupTimetable {
        GroupTimetable {
            is_due_now: true,
            due_until: None,
            next_shows_at: None,
            last_uncheck_at,
            next_uncheck_at: None,
        }
    }

    #[test]
    fn keeps_only_groups_that_have_unchecked() {
        let unchecked_group_id = Uuid::now_v7();
        let unchecked_timetable =
            timetable_with_last_uncheck(Some(UtcInstant::from_unix_millis(1_000)));
        let never_unchecked_timetable = timetable_with_last_uncheck(None);

        let uncheck_cutoffs = UncheckCutoffs::from_timetables(
            [
                (unchecked_group_id, &unchecked_timetable),
                (Uuid::now_v7(), &never_unchecked_timetable),
            ]
            .into_iter(),
        );

        assert_eq!(
            uncheck_cutoffs.cutoff_millis_for(unchecked_group_id),
            Some(1_000)
        );
        assert_eq!(uncheck_cutoffs.cutoff_millis_for(Uuid::now_v7()), None);
        assert_eq!(
            uncheck_cutoffs,
            UncheckCutoffs {
                group_ids: vec![unchecked_group_id],
                cutoff_millis: vec![1_000],
            }
        );
    }
}
