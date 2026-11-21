use std::cmp::Ordering;

use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::tasks::entities::task::TaskPriority;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanningOrder {
    pub due: Option<UtcInstant>,
    pub priority: TaskPriority,
    pub group_created_at: UtcInstant,
    pub group_id: Uuid,
    pub position: i32,
    pub task_id: Uuid,
}

impl PlanningOrder {
    fn sort_key(
        &self,
    ) -> (
        bool,
        Option<UtcInstant>,
        TaskPriority,
        UtcInstant,
        Uuid,
        i32,
        Uuid,
    ) {
        (
            self.due.is_none(),
            self.due,
            self.priority,
            self.group_created_at,
            self.group_id,
            self.position,
            self.task_id,
        )
    }
}

impl Ord for PlanningOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for PlanningOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(due_millis: Option<i64>, priority: TaskPriority, position: i32) -> PlanningOrder {
        PlanningOrder {
            due: due_millis.map(UtcInstant::from_unix_millis),
            priority,
            group_created_at: UtcInstant::from_unix_millis(0),
            group_id: Uuid::nil(),
            position,
            task_id: Uuid::nil(),
        }
    }

    #[test]
    fn puts_due_dates_first_then_priority_then_position() {
        let mut orders = vec![
            order(None, TaskPriority::High, 0),
            order(Some(2_000), TaskPriority::Low, 0),
            order(Some(1_000), TaskPriority::Low, 0),
            order(None, TaskPriority::Normal, 1),
            order(None, TaskPriority::Normal, 0),
        ];

        orders.sort();

        assert_eq!(
            orders,
            vec![
                order(Some(1_000), TaskPriority::Low, 0),
                order(Some(2_000), TaskPriority::Low, 0),
                order(None, TaskPriority::High, 0),
                order(None, TaskPriority::Normal, 0),
                order(None, TaskPriority::Normal, 1),
            ]
        );
    }
}
