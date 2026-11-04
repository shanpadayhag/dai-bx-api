use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::actions::open_running_event::OpenRunningEvent;
use crate::features::calendar_events::entities::calendar_event::{
    NewRunningEvent, RunningItem, TrackedSubject,
};
use crate::features::tasks::actions::find_trackable_task::FindTrackableTask;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tracking::error::TrackingError;

pub struct StartTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    find_trackable_task: FindTrackableTask,
    open_running_event: OpenRunningEvent,
}

impl<'a> StartTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            find_trackable_task: FindTrackableTask::new(),
            open_running_event: OpenRunningEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
    ) -> Result<RunningItem, TrackingError> {
        let mut start_transaction = self.db_pool.begin().await?;
        self.lock_group_of_task
            .execute(&mut start_transaction, user_id, task_id)
            .await?;
        let trackable_task = self
            .find_trackable_task
            .execute(&mut start_transaction, user_id, task_id)
            .await?;
        if !trackable_task.can_start() {
            return Err(TrackingError::Invalid(String::from(
                "only an open top-level task can start",
            )));
        }
        let running_item = self
            .open_running_event
            .execute(
                &mut start_transaction,
                &NewRunningEvent {
                    id: Uuid::now_v7(),
                    user_id,
                    title: trackable_task.name,
                    subject: TrackedSubject::Task(task_id),
                },
            )
            .await?;
        start_transaction.commit().await?;
        Ok(running_item)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;
    use crate::features::tasks::entities::task::GroupTasks;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    pub async fn seed_user_id(db_pool: &PgPool, email: &str) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: email.to_owned(),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    fn task_id_named(group_tasks: &GroupTasks, task_name: &str) -> Uuid {
        group_tasks
            .tasks
            .iter()
            .find(|task| task.name == task_name)
            .expect("the task should be listed")
            .id
    }

    pub async fn seed_task_and_subtask(db_pool: &PgPool, user_id: Uuid) -> (Uuid, Uuid) {
        let task_group = CreateTaskGroup::new()
            .execute(db_pool, user_id, "Today")
            .await
            .expect("the group should be created");
        let add_task = AddTask::new(db_pool);
        let group_tasks = add_task
            .execute(user_id, task_group.id, "Release", None)
            .await
            .expect("the task should be added");
        let task_id = task_id_named(&group_tasks, "Release");
        let group_tasks = add_task
            .execute(user_id, task_group.id, "Tag the build", Some(task_id))
            .await
            .expect("the subtask should be added");
        (task_id, task_id_named(&group_tasks, "Tag the build"))
    }

    #[sqlx::test]
    async fn starts_an_open_top_level_task_once(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_and_subtask(&db_pool, owner_id).await;
        let start_task = StartTask::new(&db_pool);

        let running_item = start_task
            .execute(owner_id, task_id)
            .await
            .expect("the task should start");
        let second_start = start_task.execute(owner_id, task_id).await;

        assert_eq!(running_item.event.title, "Release");
        assert!(running_item.event.is_running());
        assert!(matches!(second_start, Err(TrackingError::AlreadyRunning)));
    }

    #[sqlx::test]
    async fn refuses_a_subtask_and_another_person_s_task(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let (task_id, subtask_id) = seed_task_and_subtask(&db_pool, owner_id).await;
        let start_task = StartTask::new(&db_pool);

        let subtask_start = start_task.execute(owner_id, subtask_id).await;
        let foreign_start = start_task.execute(other_person_id, task_id).await;

        assert!(matches!(subtask_start, Err(TrackingError::Invalid(_))));
        assert!(matches!(foreign_start, Err(TrackingError::NotFound)));
    }
}
