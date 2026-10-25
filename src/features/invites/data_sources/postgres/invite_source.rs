use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Default)]
pub struct PostgresInviteSource;

impl PostgresInviteSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        code_hash: &[u8],
        valid_for_seconds: i64,
    ) -> Result<String, sqlx::Error> {
        sqlx::query_scalar!(
            r#"INSERT INTO invites (id, code_hash, expires_at)
               VALUES ($1, $2, now() + make_interval(secs => $3::float8))
               RETURNING to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI') || ' UTC' AS "expires_at_utc!""#,
            Uuid::now_v7(),
            code_hash,
            valid_for_seconds as f64,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn is_usable(
        &self,
        executor: impl PgExecutor<'_>,
        code_hash: &[u8],
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (
                   SELECT 1 FROM invites
                   WHERE code_hash = $1 AND used_at IS NULL AND expires_at > now()
               ) AS "is_usable!""#,
            code_hash,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn redeem(
        &self,
        executor: impl PgExecutor<'_>,
        code_hash: &[u8],
        redeeming_user_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "UPDATE invites
             SET used_at = now(), used_by_user_id = $2
             WHERE code_hash = $1 AND used_at IS NULL AND expires_at > now()
             RETURNING id",
            code_hash,
            redeeming_user_id,
        )
        .fetch_optional(executor)
        .await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::invites::entities::invite_code::{
        INVITE_VALIDITY_SECONDS, generate_invite_code, hash_invite_code, normalize_invite_code,
    };

    pub(crate) async fn seed_user(db_pool: &PgPool) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
             VALUES ($1, gen_random_uuid(), 'Invited Person', $1::text || '@daibx.test', 'unused', 'draft-1', now())
             RETURNING id",
        )
        .bind(Uuid::now_v7())
        .fetch_one(db_pool)
        .await
        .expect("seed user insert should succeed")
    }

    fn fresh_code_hash() -> [u8; 32] {
        let invite_code = generate_invite_code();
        hash_invite_code(&normalize_invite_code(&invite_code).expect("generated code normalizes"))
    }

    #[sqlx::test]
    async fn stores_an_invite_that_expires_in_seven_days(db_pool: PgPool) {
        let expires_at_utc = PostgresInviteSource
            .insert(&db_pool, &fresh_code_hash(), INVITE_VALIDITY_SECONDS)
            .await
            .expect("insert should succeed");

        assert!(expires_at_utc.ends_with(" UTC"));
        assert_eq!(expires_at_utc.len(), "2026-10-31 12:00 UTC".len());
    }

    #[sqlx::test]
    async fn redeems_a_fresh_invite_once(db_pool: PgPool) {
        let code_hash = fresh_code_hash();
        let user_id = seed_user(&db_pool).await;
        PostgresInviteSource
            .insert(&db_pool, &code_hash, INVITE_VALIDITY_SECONDS)
            .await
            .expect("insert should succeed");

        let first_redemption = PostgresInviteSource
            .redeem(&db_pool, &code_hash, user_id)
            .await
            .expect("redeem should run");
        let second_redemption = PostgresInviteSource
            .redeem(&db_pool, &code_hash, user_id)
            .await
            .expect("redeem should run");

        assert!(first_redemption.is_some());
        assert_eq!(second_redemption, None);
    }

    #[sqlx::test]
    async fn refuses_an_expired_or_unknown_invite(db_pool: PgPool) {
        let expired_code_hash = fresh_code_hash();
        let user_id = seed_user(&db_pool).await;
        PostgresInviteSource
            .insert(&db_pool, &expired_code_hash, -60)
            .await
            .expect("insert should succeed");

        assert_eq!(
            PostgresInviteSource
                .redeem(&db_pool, &expired_code_hash, user_id)
                .await
                .expect("redeem should run"),
            None
        );
        assert_eq!(
            PostgresInviteSource
                .redeem(&db_pool, &fresh_code_hash(), user_id)
                .await
                .expect("redeem should run"),
            None
        );
    }
}
