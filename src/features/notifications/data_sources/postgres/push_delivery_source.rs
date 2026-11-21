use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryClaim<'a> {
    pub device_id: Uuid,
    pub user_id: Uuid,
    pub kind: &'a str,
    pub subject_key: &'a str,
    pub due_at: UtcInstant,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PostgresPushDeliverySource;

impl PostgresPushDeliverySource {
    pub async fn claim(
        &self,
        executor: impl PgExecutor<'_>,
        delivery_claim: &DeliveryClaim<'_>,
    ) -> Result<bool, sqlx::Error> {
        let claimed_row = sqlx::query_scalar!(
            r#"INSERT INTO push_deliveries (device_id, user_id, kind, subject_key, due_at)
               VALUES ($1, $2, $3, $4, 'epoch'::timestamptz + $5::bigint * interval '1 millisecond')
               ON CONFLICT DO NOTHING
               RETURNING 1 AS "claimed!""#,
            delivery_claim.device_id,
            delivery_claim.user_id,
            delivery_claim.kind,
            delivery_claim.subject_key,
            delivery_claim.due_at.unix_millis(),
        )
        .fetch_optional(executor)
        .await?;
        Ok(claimed_row.is_some())
    }

    pub async fn prune_sent_before(
        &self,
        executor: impl PgExecutor<'_>,
        cutoff: UtcInstant,
    ) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query!(
            "DELETE FROM push_deliveries
             WHERE sent_at < 'epoch'::timestamptz + $1::bigint * interval '1 millisecond'",
            cutoff.unix_millis(),
        )
        .execute(executor)
        .await?
        .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
    use crate::features::notifications::data_sources::postgres::push_device_source::test_support::seed_recipient;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("test instants are valid")
    }

    async fn seed_device_id(db_pool: &PgPool, email: &str) -> (Uuid, Uuid) {
        let user_id = seed_recipient(db_pool, email).await;
        let device_id = PostgresPushDeviceSource
            .list_for_user(db_pool, user_id)
            .await
            .expect("devices should list")[0]
            .id;
        (user_id, device_id)
    }

    #[sqlx::test]
    async fn claims_a_delivery_only_once(db_pool: PgPool) {
        let (user_id, device_id) = seed_device_id(&db_pool, "phone@example.com").await;
        let delivery_claim = DeliveryClaim {
            device_id,
            user_id,
            kind: "check_in",
            subject_key: "event",
            due_at: instant("2026-11-09T10:00:00Z"),
        };
        let source = PostgresPushDeliverySource;

        let first_claim = source.claim(&db_pool, &delivery_claim).await;
        let second_claim = source.claim(&db_pool, &delivery_claim).await;
        let next_due_claim = source
            .claim(
                &db_pool,
                &DeliveryClaim {
                    due_at: instant("2026-11-09T10:30:00Z"),
                    ..delivery_claim.clone()
                },
            )
            .await;

        assert_eq!(first_claim.ok(), Some(true));
        assert_eq!(second_claim.ok(), Some(false));
        assert_eq!(next_due_claim.ok(), Some(true));
    }

    #[sqlx::test]
    async fn prunes_only_deliveries_sent_before_the_cutoff(db_pool: PgPool) {
        let (user_id, device_id) = seed_device_id(&db_pool, "phone@example.com").await;
        for (subject_key, sent_at) in [
            ("old", "2026-11-07T09:59:00Z"),
            ("recent", "2026-11-07T10:01:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO push_deliveries (device_id, user_id, kind, subject_key, due_at, sent_at)
                 VALUES ($1, $2, 'check_in', $3, $4::timestamptz, $4::timestamptz)",
            )
            .bind(device_id)
            .bind(user_id)
            .bind(subject_key)
            .bind(sent_at)
            .execute(&db_pool)
            .await
            .expect("the delivery should be inserted");
        }

        let pruned_count = PostgresPushDeliverySource
            .prune_sent_before(&db_pool, instant("2026-11-07T10:00:00Z"))
            .await
            .expect("pruning should run");

        let remaining_subjects: Vec<String> =
            sqlx::query_scalar("SELECT subject_key FROM push_deliveries")
                .fetch_all(&db_pool)
                .await
                .expect("deliveries should be readable");
        assert_eq!(pruned_count, 1);
        assert_eq!(remaining_subjects, vec![String::from("recent")]);
    }
}
