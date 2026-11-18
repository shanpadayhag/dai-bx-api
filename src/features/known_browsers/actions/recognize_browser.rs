use sqlx::PgConnection;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::opaque_token::{generate_opaque_token, hash_opaque_token};
use crate::features::known_browsers::data_sources::postgres::known_browser_source::PostgresKnownBrowserSource;
use crate::features::known_browsers::entities::known_browser::KNOWN_BROWSER_VALID_FOR_SECONDS;
use crate::features::known_browsers::error::KnownBrowserError;

pub struct RecognizedBrowser {
    pub browser_token: Zeroizing<String>,
    pub was_already_known: bool,
}

#[derive(Default)]
pub struct RecognizeBrowser {
    postgres_known_browser_source: PostgresKnownBrowserSource,
}

impl RecognizeBrowser {
    pub fn new() -> Self {
        Self {
            postgres_known_browser_source: PostgresKnownBrowserSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        presented_browser_token: Option<&str>,
    ) -> Result<RecognizedBrowser, KnownBrowserError> {
        if let Some(presented_browser_token) = presented_browser_token {
            let was_already_known = self
                .postgres_known_browser_source
                .extend_active(
                    &mut *connection,
                    user_id,
                    &hash_opaque_token(presented_browser_token),
                    KNOWN_BROWSER_VALID_FOR_SECONDS,
                )
                .await?;
            if was_already_known {
                return Ok(RecognizedBrowser {
                    browser_token: Zeroizing::new(presented_browser_token.to_owned()),
                    was_already_known,
                });
            }
        }
        let browser_token = generate_opaque_token();
        self.postgres_known_browser_source
            .insert(
                connection,
                user_id,
                &hash_opaque_token(&browser_token),
                KNOWN_BROWSER_VALID_FOR_SECONDS,
            )
            .await?;
        Ok(RecognizedBrowser {
            browser_token,
            was_already_known: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_user_id(db_pool: &PgPool, email: &str) -> Uuid {
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

    async fn recognize(
        db_pool: &PgPool,
        user_id: Uuid,
        presented_browser_token: Option<&str>,
    ) -> RecognizedBrowser {
        let mut connection = db_pool.acquire().await.expect("a connection");
        RecognizeBrowser::new()
            .execute(&mut connection, user_id, presented_browser_token)
            .await
            .expect("recognizing should succeed")
    }

    #[sqlx::test]
    async fn issues_a_token_and_recognizes_it_next_time(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "person@example.com").await;

        let new_browser = recognize(&db_pool, user_id, None).await;
        let known_browser = recognize(&db_pool, user_id, Some(&new_browser.browser_token)).await;

        assert!(!new_browser.was_already_known);
        assert!(known_browser.was_already_known);
        assert_eq!(*known_browser.browser_token, *new_browser.browser_token);
    }

    #[sqlx::test]
    async fn does_not_recognize_an_unknown_token_or_another_accounts_token(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "person@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_browser = recognize(&db_pool, other_user_id, None).await;

        for presented_browser_token in ["unknown-browser-token", &other_browser.browser_token] {
            let recognized_browser =
                recognize(&db_pool, user_id, Some(presented_browser_token)).await;

            assert!(!recognized_browser.was_already_known);
            assert_ne!(*recognized_browser.browser_token, presented_browser_token);
        }
    }
}
