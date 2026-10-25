use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::clock::current_unix_seconds;
use crate::core::security::totp::find_accepted_totp_step;
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::error::UserError;

pub struct AcceptTotpCode<'a> {
    totp_keyring: &'a TotpKeyring,
    postgres_user_source: PostgresUserSource,
}

impl<'a> AcceptTotpCode<'a> {
    pub fn new(totp_keyring: &'a TotpKeyring) -> Self {
        Self {
            totp_keyring,
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        entered_code: &str,
    ) -> Result<bool, UserError> {
        let Some(totp_state) = self
            .postgres_user_source
            .find_totp_state_for_update(&mut *connection, user_id)
            .await?
        else {
            return Ok(false);
        };
        let Some(sealed_totp_secret) = totp_state.sealed_totp_secret else {
            return Ok(false);
        };
        let totp_secret = self.totp_keyring.open(user_id, &sealed_totp_secret)?;
        let last_used_step = totp_state
            .last_used_step
            .and_then(|used_step| u64::try_from(used_step).ok());
        let Some(accepted_totp_step) = find_accepted_totp_step(
            &totp_secret,
            entered_code,
            current_unix_seconds(),
            last_used_step,
        ) else {
            return Ok(false);
        };
        let accepted_totp_step =
            i64::try_from(accepted_totp_step).expect("TOTP steps stay far below i64::MAX");
        Ok(self
            .postgres_user_source
            .advance_totp_step(&mut *connection, user_id, accepted_totp_step)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::security::totp::{generate_totp_secret, totp_code_at_step, totp_step_at};
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::actions::enable_mfa::EnableMfa;
    use crate::features::users::entities::user::NewUser;

    fn test_keyring() -> TotpKeyring {
        TotpKeyring::parse("v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "v1")
            .expect("test keyring should parse")
    }

    #[sqlx::test]
    async fn accepts_each_code_once_and_never_an_older_one(db_pool: PgPool) {
        let totp_keyring = test_keyring();
        let user_id = CreateUser::new()
            .execute(
                &db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: String::from("person@example.com"),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id;
        let totp_secret = generate_totp_secret();
        let current_step = totp_step_at(current_unix_seconds());
        EnableMfa::new()
            .execute(
                &db_pool,
                user_id,
                &totp_keyring.seal(user_id, &totp_secret),
                current_step - 1,
            )
            .await
            .expect("enabling should succeed");
        let mut connection = db_pool.acquire().await.expect("a connection");
        let accept_totp_code = AcceptTotpCode::new(&totp_keyring);

        let current_code = totp_code_at_step(&totp_secret, current_step);
        let older_code = totp_code_at_step(&totp_secret, current_step - 1);

        assert!(
            accept_totp_code
                .execute(&mut connection, user_id, &current_code)
                .await
                .expect("checking should run")
        );
        assert!(
            !accept_totp_code
                .execute(&mut connection, user_id, &current_code)
                .await
                .expect("checking should run")
        );
        assert!(
            !accept_totp_code
                .execute(&mut connection, user_id, &older_code)
                .await
                .expect("checking should run")
        );
    }
}
