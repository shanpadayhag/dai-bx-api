use sqlx::PgExecutor;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::opaque_token::{generate_opaque_token, hash_opaque_token};
use crate::features::remembered_devices::data_sources::postgres::remembered_device_source::PostgresRememberedDeviceSource;
use crate::features::remembered_devices::entities::remembered_device::REMEMBERED_DEVICE_VALID_FOR_SECONDS;
use crate::features::remembered_devices::error::RememberedDeviceError;

#[derive(Default)]
pub struct RememberDevice {
    postgres_remembered_device_source: PostgresRememberedDeviceSource,
}

impl RememberDevice {
    pub fn new() -> Self {
        Self {
            postgres_remembered_device_source: PostgresRememberedDeviceSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        user_agent: Option<&str>,
    ) -> Result<Zeroizing<String>, RememberedDeviceError> {
        let device_token = generate_opaque_token();
        self.postgres_remembered_device_source
            .insert(
                executor,
                user_id,
                &hash_opaque_token(&device_token),
                user_agent,
                REMEMBERED_DEVICE_VALID_FOR_SECONDS,
            )
            .await?;
        Ok(device_token)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::remembered_devices::actions::find_remembered_device::FindRememberedDevice;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_user_id(db_pool: &PgPool) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
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
            .id
    }

    #[sqlx::test]
    async fn remembers_a_device_that_can_be_found_by_its_token(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;

        let device_token = RememberDevice::new()
            .execute(&db_pool, user_id, Some("DaiBX test"))
            .await
            .expect("remembering should succeed");
        let remembered_device = FindRememberedDevice::new()
            .execute(&db_pool, &device_token)
            .await
            .expect("lookup should succeed")
            .expect("the device should be remembered");

        assert_eq!(remembered_device.user_id, user_id);
        assert_eq!(
            FindRememberedDevice::new()
                .execute(&db_pool, "unknown-device-token")
                .await
                .expect("lookup should succeed"),
            None
        );
    }
}
