use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::notifications::entities::push_device::{NewPushDevice, PushDevice};
use crate::features::notifications::entities::push_device_keys::PushDeviceKeys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointOwner {
    pub device_id: Uuid,
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedPushDevice {
    pub device_id: Uuid,
    pub user_id: Uuid,
}

struct PushDeviceRow {
    id: Uuid,
    user_id: Uuid,
    endpoint: String,
    p256dh: String,
    auth_secret: String,
    device_name: String,
    time_zone: String,
    created_at: String,
    last_delivered_at: Option<String>,
}

impl From<PushDeviceRow> for PushDevice {
    fn from(push_device_row: PushDeviceRow) -> Self {
        Self {
            id: push_device_row.id,
            user_id: push_device_row.user_id,
            endpoint: push_device_row.endpoint,
            keys: PushDeviceKeys {
                p256dh: push_device_row.p256dh,
                auth_secret: push_device_row.auth_secret,
            },
            device_name: push_device_row.device_name,
            time_zone: push_device_row.time_zone,
            created_at: push_device_row.created_at,
            last_delivered_at: push_device_row.last_delivered_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PostgresPushDeviceSource;

impl PostgresPushDeviceSource {
    pub async fn lock_owner(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        Ok(
            sqlx::query_scalar!("SELECT id FROM users WHERE id = $1 FOR UPDATE", user_id)
                .fetch_optional(executor)
                .await?
                .is_some(),
        )
    }

    pub async fn find_endpoint_owner(
        &self,
        executor: impl PgExecutor<'_>,
        endpoint: &str,
    ) -> Result<Option<EndpointOwner>, sqlx::Error> {
        sqlx::query_as!(
            EndpointOwner,
            r#"SELECT id AS device_id, user_id FROM push_devices WHERE endpoint = $1 FOR UPDATE"#,
            endpoint
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn count_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*) AS "device_count!" FROM push_devices WHERE user_id = $1"#,
            user_id
        )
        .fetch_one(executor)
        .await
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_push_device: &NewPushDevice,
    ) -> Result<PushDevice, sqlx::Error> {
        sqlx::query_as!(
            PushDeviceRow,
            r#"INSERT INTO push_devices (id, user_id, session_id, endpoint, p256dh, auth_secret, device_name, time_zone)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id, user_id, endpoint, p256dh, auth_secret, device_name, time_zone,
                         to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "created_at!",
                         to_char(last_delivered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_delivered_at"#,
            Uuid::now_v7(),
            new_push_device.user_id,
            new_push_device.session_id,
            new_push_device.endpoint.as_str(),
            new_push_device.keys.p256dh,
            new_push_device.keys.auth_secret,
            new_push_device.device_name,
            new_push_device.time_zone
        )
        .fetch_one(executor)
        .await
        .map(PushDevice::from)
    }

    pub async fn update_registration(
        &self,
        executor: impl PgExecutor<'_>,
        device_id: Uuid,
        new_push_device: &NewPushDevice,
    ) -> Result<PushDevice, sqlx::Error> {
        sqlx::query_as!(
            PushDeviceRow,
            r#"UPDATE push_devices
               SET session_id = $2, p256dh = $3, auth_secret = $4, device_name = $5, time_zone = $6
               WHERE id = $1
               RETURNING id, user_id, endpoint, p256dh, auth_secret, device_name, time_zone,
                         to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "created_at!",
                         to_char(last_delivered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_delivered_at"#,
            device_id,
            new_push_device.session_id,
            new_push_device.keys.p256dh,
            new_push_device.keys.auth_secret,
            new_push_device.device_name,
            new_push_device.time_zone
        )
        .fetch_one(executor)
        .await
        .map(PushDevice::from)
    }

    pub async fn list_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<PushDevice>, sqlx::Error> {
        Ok(sqlx::query_as!(
            PushDeviceRow,
            r#"SELECT id, user_id, endpoint, p256dh, auth_secret, device_name, time_zone,
                      to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "created_at!",
                      to_char(last_delivered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_delivered_at
               FROM push_devices
               WHERE user_id = $1
               ORDER BY created_at, id"#,
            user_id
        )
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(PushDevice::from)
        .collect())
    }

    pub async fn list_for_users(
        &self,
        executor: impl PgExecutor<'_>,
        user_ids: &[Uuid],
    ) -> Result<Vec<PushDevice>, sqlx::Error> {
        Ok(sqlx::query_as!(
            PushDeviceRow,
            r#"SELECT id, user_id, endpoint, p256dh, auth_secret, device_name, time_zone,
                      to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "created_at!",
                      to_char(last_delivered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_delivered_at
               FROM push_devices
               WHERE user_id = ANY($1)
               ORDER BY user_id, created_at, id"#,
            user_ids
        )
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(PushDevice::from)
        .collect())
    }

    pub async fn find_owned(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Option<PushDevice>, sqlx::Error> {
        Ok(sqlx::query_as!(
            PushDeviceRow,
            r#"SELECT id, user_id, endpoint, p256dh, auth_secret, device_name, time_zone,
                      to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "created_at!",
                      to_char(last_delivered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_delivered_at
               FROM push_devices
               WHERE id = $1 AND user_id = $2"#,
            device_id,
            user_id
        )
        .fetch_optional(executor)
        .await?
        .map(PushDevice::from))
    }

    pub async fn delete_owned(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query!(
            "DELETE FROM push_devices WHERE id = $1 AND user_id = $2",
            device_id,
            user_id
        )
        .execute(executor)
        .await?
        .rows_affected()
            == 1)
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        device_id: Uuid,
    ) -> Result<Option<RemovedPushDevice>, sqlx::Error> {
        sqlx::query_as!(
            RemovedPushDevice,
            "DELETE FROM push_devices WHERE id = $1 RETURNING id AS device_id, user_id",
            device_id
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn mark_delivered(
        &self,
        executor: impl PgExecutor<'_>,
        device_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE push_devices SET last_delivered_at = now() WHERE id = $1",
            device_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod test_support {
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;
    use crate::features::notifications::entities::push_endpoint::PushEndpoint;

    pub const SAMPLE_P256DH: &str =
        "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
    pub const SAMPLE_AUTH_SECRET: &str = "tBHItJI5svbpez7KI4CCXg";

    pub async fn seed_user_with_session(db_pool: &PgPool, email: &str) -> (Uuid, Uuid) {
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
             VALUES (gen_random_uuid(), gen_random_uuid(), 'Test Person', $1, '$argon2id$placeholder', 'draft-1', now())
             RETURNING id",
        )
        .bind(email)
        .fetch_one(db_pool)
        .await
        .expect("seed user insert should succeed");
        let session_id: Uuid = sqlx::query_scalar(
            "INSERT INTO sessions (id, user_id) VALUES (gen_random_uuid(), $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(db_pool)
        .await
        .expect("seed session insert should succeed");
        (user_id, session_id)
    }

    pub async fn seed_recipient(db_pool: &PgPool, email: &str) -> Uuid {
        let (user_id, session_id) = seed_user_with_session(db_pool, email).await;
        PostgresPushDeviceSource
            .insert(
                db_pool,
                &new_device(user_id, session_id, &email.replace('@', "-")),
            )
            .await
            .expect("seed device insert should succeed");
        user_id
    }

    pub fn new_device(user_id: Uuid, session_id: Uuid, endpoint_suffix: &str) -> NewPushDevice {
        NewPushDevice {
            user_id,
            session_id,
            endpoint: PushEndpoint::parse(&format!(
                "https://fcm.googleapis.com/fcm/send/{endpoint_suffix}"
            ))
            .expect("sample endpoint should parse"),
            keys: PushDeviceKeys {
                p256dh: String::from(SAMPLE_P256DH),
                auth_secret: String::from(SAMPLE_AUTH_SECRET),
            },
            device_name: String::from("iPhone · Safari"),
            time_zone: String::from("Asia/Manila"),
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::test_support::{new_device, seed_user_with_session};
    use super::*;

    #[sqlx::test]
    async fn lists_the_devices_of_the_given_users_only(db_pool: PgPool) {
        let (user_id, session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        let (other_user_id, other_session_id) =
            seed_user_with_session(&db_pool, "b@example.com").await;
        let (left_out_user_id, left_out_session_id) =
            seed_user_with_session(&db_pool, "c@example.com").await;
        let source = PostgresPushDeviceSource;
        for (owner_id, owner_session_id, endpoint_suffix) in [
            (user_id, session_id, "a"),
            (other_user_id, other_session_id, "b"),
            (left_out_user_id, left_out_session_id, "c"),
        ] {
            source
                .insert(
                    &db_pool,
                    &new_device(owner_id, owner_session_id, endpoint_suffix),
                )
                .await
                .expect("insert should succeed");
        }

        let listed_devices = source
            .list_for_users(&db_pool, &[user_id, other_user_id])
            .await
            .expect("list should succeed");

        let mut listed_owner_ids: Vec<Uuid> = listed_devices
            .iter()
            .map(|listed_device| listed_device.user_id)
            .collect();
        listed_owner_ids.sort();
        let mut expected_owner_ids = vec![user_id, other_user_id];
        expected_owner_ids.sort();
        assert_eq!(listed_owner_ids, expected_owner_ids);
    }

    #[sqlx::test]
    async fn lists_only_the_owners_devices_oldest_first(db_pool: PgPool) {
        let (owner_id, owner_session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        let (other_id, other_session_id) = seed_user_with_session(&db_pool, "b@example.com").await;
        let source = PostgresPushDeviceSource;
        let first_device = source
            .insert(&db_pool, &new_device(owner_id, owner_session_id, "first"))
            .await
            .expect("insert should succeed");
        let second_device = source
            .insert(&db_pool, &new_device(owner_id, owner_session_id, "second"))
            .await
            .expect("insert should succeed");
        source
            .insert(&db_pool, &new_device(other_id, other_session_id, "other"))
            .await
            .expect("insert should succeed");

        let owner_devices = source
            .list_for_user(&db_pool, owner_id)
            .await
            .expect("list should succeed");

        assert_eq!(owner_devices, vec![first_device, second_device]);
        assert_eq!(
            source.count_for_user(&db_pool, owner_id).await.ok(),
            Some(2)
        );
    }

    #[sqlx::test]
    async fn finds_and_deletes_a_device_only_for_its_owner(db_pool: PgPool) {
        let (owner_id, owner_session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        let (other_id, _) = seed_user_with_session(&db_pool, "b@example.com").await;
        let source = PostgresPushDeviceSource;
        let owner_device = source
            .insert(&db_pool, &new_device(owner_id, owner_session_id, "first"))
            .await
            .expect("insert should succeed");

        assert_eq!(
            source
                .find_owned(&db_pool, other_id, owner_device.id)
                .await
                .ok(),
            Some(None)
        );
        assert_eq!(
            source
                .delete_owned(&db_pool, other_id, owner_device.id)
                .await
                .ok(),
            Some(false)
        );
        assert_eq!(
            source
                .delete_owned(&db_pool, owner_id, owner_device.id)
                .await
                .ok(),
            Some(true)
        );
    }
}
