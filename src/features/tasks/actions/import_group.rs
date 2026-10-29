use sqlx::PgPool;
use uuid::Uuid;

use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::group_import::GroupImport;
use crate::features::tasks::entities::task::{GroupTasks, NewTask};
use crate::features::tasks::error::TaskError;

pub struct ImportGroup<'a> {
    db_pool: &'a PgPool,
    create_task_group: CreateTaskGroup,
    postgres_task_source: PostgresTaskSource,
}

fn new_tasks_for(group_import: &GroupImport, user_id: Uuid, task_group_id: Uuid) -> Vec<NewTask> {
    let task_ids: Vec<Uuid> = group_import.tasks.iter().map(|_| Uuid::now_v7()).collect();
    group_import
        .tasks
        .iter()
        .zip(&task_ids)
        .map(|(imported_task, task_id)| NewTask {
            id: *task_id,
            user_id,
            group_id: task_group_id,
            parent_id: imported_task
                .parent_index
                .and_then(|parent_index| task_ids.get(parent_index).copied()),
            position: imported_task.position,
            name: imported_task.name.clone(),
        })
        .collect()
}

impl<'a> ImportGroup<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            create_task_group: CreateTaskGroup::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        group_import: &GroupImport,
    ) -> Result<GroupTasks, TaskError> {
        let mut import_transaction = self.db_pool.begin().await?;
        let created_task_group = self
            .create_task_group
            .execute(&mut *import_transaction, user_id, &group_import.name)
            .await?;
        let new_tasks = new_tasks_for(group_import, user_id, created_task_group.id);
        self.postgres_task_source
            .insert_many(
                &mut *import_transaction,
                user_id,
                created_task_group.id,
                &new_tasks,
            )
            .await?;
        let group_tasks = self
            .postgres_task_source
            .list_for_group(&mut *import_transaction, created_task_group.id)
            .await?;
        import_transaction.commit().await?;
        Ok(GroupTasks {
            group: TaskGroupDetails {
                id: created_task_group.id,
                name: created_task_group.name,
            },
            tasks: group_tasks,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::tasks::data_sources::postgres::task_source::tests::seed_user_id;
    use crate::features::tasks::entities::group_import::parse_group_import;

    #[sqlx::test]
    async fn links_each_imported_task_to_its_parent(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let group_import = parse_group_import(
            br#"{"name":"Today","tasks":[{"name":"Card","tasks":[{"name":"Step","tasks":[]}]}]}"#,
        )
        .expect("the file should parse");

        let group_tasks = ImportGroup::new(&db_pool)
            .execute(owner_id, &group_import)
            .await
            .expect("the import should succeed");

        let card_task = group_tasks
            .tasks
            .iter()
            .find(|task| task.name == "Card")
            .expect("the card task should be imported");
        let step_task = group_tasks
            .tasks
            .iter()
            .find(|task| task.name == "Step")
            .expect("the step task should be imported");
        assert_eq!(step_task.parent_id, Some(card_task.id));
        assert_eq!(group_tasks.group.name, "Today");
    }
}
