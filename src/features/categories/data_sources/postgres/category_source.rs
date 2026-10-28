use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::entities::category::{
    Category, CategoryChanges, CategorySave, NewCategory,
};

#[derive(Default)]
pub struct PostgresCategorySource;

fn save_outcome(
    save_result: Result<Option<Category>, sqlx::Error>,
) -> Result<Option<CategorySave>, sqlx::Error> {
    match save_result {
        Ok(saved_category) => Ok(saved_category.map(CategorySave::Saved)),
        Err(sqlx::Error::Database(database_error)) if database_error.is_unique_violation() => {
            Ok(Some(CategorySave::NameTaken))
        }
        Err(other_error) => Err(other_error),
    }
}

impl PostgresCategorySource {
    pub async fn list_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<Category>, sqlx::Error> {
        sqlx::query_as!(
            Category,
            r#"SELECT id, name::text AS "name!", color
               FROM categories
               WHERE user_id = $1
               ORDER BY created_at, id"#,
            user_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_category: &NewCategory,
    ) -> Result<CategorySave, sqlx::Error> {
        let insert_result = sqlx::query_as!(
            Category,
            r#"INSERT INTO categories (id, user_id, name, color)
               VALUES ($1, $2, $3::text::citext, $4)
               RETURNING id, name::text AS "name!", color"#,
            new_category.id,
            new_category.user_id,
            new_category.name,
            new_category.color,
        )
        .fetch_one(executor)
        .await
        .map(Some);
        Ok(save_outcome(insert_result)?.unwrap_or(CategorySave::NameTaken))
    }

    pub async fn insert_many(
        &self,
        executor: impl PgExecutor<'_>,
        new_categories: &[NewCategory],
    ) -> Result<(), sqlx::Error> {
        let category_ids: Vec<Uuid> = new_categories.iter().map(|category| category.id).collect();
        let user_ids: Vec<Uuid> = new_categories
            .iter()
            .map(|category| category.user_id)
            .collect();
        let names: Vec<String> = new_categories
            .iter()
            .map(|category| category.name.clone())
            .collect();
        let colors: Vec<String> = new_categories
            .iter()
            .map(|category| category.color.clone())
            .collect();
        sqlx::query!(
            r#"INSERT INTO categories (id, user_id, name, color)
               SELECT id, user_id, name::citext, color
               FROM UNNEST($1::uuid[], $2::uuid[], $3::text[], $4::text[])
                    AS new_category (id, user_id, name, color)"#,
            &category_ids,
            &user_ids,
            &names,
            &colors,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        category_id: Uuid,
        category_changes: &CategoryChanges,
    ) -> Result<Option<CategorySave>, sqlx::Error> {
        let update_result = sqlx::query_as!(
            Category,
            r#"UPDATE categories
               SET name = coalesce($3::text::citext, name),
                   color = coalesce($4, color),
                   updated_at = now()
               WHERE id = $1 AND user_id = $2
               RETURNING id, name::text AS "name!", color"#,
            category_id,
            user_id,
            category_changes.name.as_deref(),
            category_changes.color.as_deref(),
        )
        .fetch_optional(executor)
        .await;
        save_outcome(update_result)
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        category_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let delete_outcome = sqlx::query!(
            "DELETE FROM categories WHERE id = $1 AND user_id = $2",
            category_id,
            user_id,
        )
        .execute(executor)
        .await?;
        Ok(delete_outcome.rows_affected() == 1)
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
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
            .expect("creating the user should run")
            .expect("the email should be free")
            .id
    }

    fn new_category(user_id: Uuid, name: &str) -> NewCategory {
        NewCategory {
            id: Uuid::now_v7(),
            user_id,
            name: name.to_owned(),
            color: String::from("#8fae9a"),
        }
    }

    #[sqlx::test]
    async fn lists_only_the_owner_s_categories_oldest_first(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let category_source = PostgresCategorySource;
        for category_name in ["Health", "Chores"] {
            category_source
                .insert(&db_pool, &new_category(owner_id, category_name))
                .await
                .expect("insert should run");
        }
        category_source
            .insert(&db_pool, &new_category(other_person_id, "Hidden"))
            .await
            .expect("insert should run");

        let listed_names: Vec<String> = category_source
            .list_for_user(&db_pool, owner_id)
            .await
            .expect("listing should run")
            .into_iter()
            .map(|category| category.name)
            .collect();

        assert_eq!(listed_names, vec!["Health", "Chores"]);
    }

    #[sqlx::test]
    async fn reports_a_taken_name_on_insert_and_update(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_source = PostgresCategorySource;
        category_source
            .insert(&db_pool, &new_category(owner_id, "Health"))
            .await
            .expect("insert should run");
        let CategorySave::Saved(chores_category) = category_source
            .insert(&db_pool, &new_category(owner_id, "Chores"))
            .await
            .expect("insert should run")
        else {
            panic!("Chores should be saved");
        };

        let duplicate_insert = category_source
            .insert(&db_pool, &new_category(owner_id, "HEALTH"))
            .await
            .expect("insert should run");
        let duplicate_rename = category_source
            .update(
                &db_pool,
                owner_id,
                chores_category.id,
                &CategoryChanges {
                    name: Some(String::from("health")),
                    color: None,
                },
            )
            .await
            .expect("update should run");

        assert_eq!(duplicate_insert, CategorySave::NameTaken);
        assert_eq!(duplicate_rename, Some(CategorySave::NameTaken));
    }

    #[sqlx::test]
    async fn updates_and_deletes_only_the_owner_s_category(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let category_source = PostgresCategorySource;
        let CategorySave::Saved(health_category) = category_source
            .insert(&db_pool, &new_category(owner_id, "Health"))
            .await
            .expect("insert should run")
        else {
            panic!("Health should be saved");
        };
        let recolor = CategoryChanges {
            name: None,
            color: Some(String::from("#000000")),
        };

        let other_person_update = category_source
            .update(&db_pool, other_person_id, health_category.id, &recolor)
            .await
            .expect("update should run");
        let other_person_delete = category_source
            .delete(&db_pool, other_person_id, health_category.id)
            .await
            .expect("delete should run");
        let owner_update = category_source
            .update(&db_pool, owner_id, health_category.id, &recolor)
            .await
            .expect("update should run");
        let owner_delete = category_source
            .delete(&db_pool, owner_id, health_category.id)
            .await
            .expect("delete should run");

        assert_eq!(other_person_update, None);
        assert!(!other_person_delete);
        assert_eq!(
            owner_update,
            Some(CategorySave::Saved(Category {
                id: health_category.id,
                name: String::from("Health"),
                color: String::from("#000000"),
            }))
        );
        assert!(owner_delete);
    }

    #[sqlx::test]
    async fn inserts_many_categories_in_one_statement(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_source = PostgresCategorySource;

        category_source
            .insert_many(
                &db_pool,
                &[new_category(owner_id, "One"), new_category(owner_id, "Two")],
            )
            .await
            .expect("insert should run");

        assert_eq!(
            category_source
                .list_for_user(&db_pool, owner_id)
                .await
                .expect("listing should run")
                .len(),
            2
        );
    }
}
