use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::known_browsers::data_sources::postgres::known_browser_source::PostgresKnownBrowserSource;
use crate::features::known_browsers::error::KnownBrowserError;

#[derive(Default)]
pub struct ForgetKnownBrowsers {
    postgres_known_browser_source: PostgresKnownBrowserSource,
}

impl ForgetKnownBrowsers {
    pub fn new() -> Self {
        Self {
            postgres_known_browser_source: PostgresKnownBrowserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<(), KnownBrowserError> {
        Ok(self
            .postgres_known_browser_source
            .delete_all_for_user(executor, user_id)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::known_browsers::actions::recognize_browser::RecognizeBrowser;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_known_browser(db_pool: &PgPool, email: &str) -> (Uuid, String) {
        let user_id = CreateUser::new()
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
            .id;
        let mut connection = db_pool.acquire().await.expect("a connection");
        let recognized_browser = RecognizeBrowser::new()
            .execute(&mut connection, user_id, None)
            .await
            .expect("recognizing should succeed");
        (user_id, recognized_browser.browser_token.to_string())
    }

    async fn is_known(db_pool: &PgPool, user_id: Uuid, browser_token: &str) -> bool {
        let mut connection = db_pool.acquire().await.expect("a connection");
        RecognizeBrowser::new()
            .execute(&mut connection, user_id, Some(browser_token))
            .await
            .expect("recognizing should succeed")
            .was_already_known
    }

    #[sqlx::test]
    async fn forgets_only_that_accounts_browsers(db_pool: PgPool) {
        let (user_id, browser_token) = seed_known_browser(&db_pool, "person@example.com").await;
        let (other_user_id, other_browser_token) =
            seed_known_browser(&db_pool, "other@example.com").await;

        ForgetKnownBrowsers::new()
            .execute(&db_pool, user_id)
            .await
            .expect("forgetting should succeed");

        assert!(!is_known(&db_pool, user_id, &browser_token).await);
        assert!(is_known(&db_pool, other_user_id, &other_browser_token).await);
    }
}
