use std::net::IpAddr;

use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::sessions::entities::session::{
    ActiveSession, PresentedRefreshToken, SessionTokenKind,
};

#[derive(Default)]
pub struct PostgresSessionSource;

impl PostgresSessionSource {
    pub async fn insert_session(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        client_ip: Option<IpAddr>,
        user_agent: Option<&str>,
    ) -> Result<Uuid, sqlx::Error> {
        let session_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO sessions (id, user_id, ip, user_agent) VALUES ($1, $2, $3, $4)",
            session_id,
            user_id,
            client_ip as Option<IpAddr>,
            user_agent,
        )
        .execute(executor)
        .await?;
        Ok(session_id)
    }

    pub async fn insert_token(
        &self,
        executor: impl PgExecutor<'_>,
        session_id: Uuid,
        kind: SessionTokenKind,
        token_hash: &[u8],
        absolute_limit_seconds: i64,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(
            r#"INSERT INTO session_tokens (id, session_id, kind, token_hash, expires_at)
               SELECT $1, sessions.id, $3, $4,
                      least(now() + make_interval(secs => $5::float8),
                            sessions.created_at + make_interval(secs => $6::float8))
               FROM sessions
               WHERE sessions.id = $2
               RETURNING ceil(extract(epoch FROM expires_at - now()))::bigint AS "valid_for_seconds!""#,
            Uuid::now_v7(),
            session_id,
            kind as SessionTokenKind,
            token_hash,
            kind.valid_for_seconds() as f64,
            absolute_limit_seconds as f64,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn find_refresh_token_for_update(
        &self,
        executor: impl PgExecutor<'_>,
        token_hash: &[u8],
        idle_limit_seconds: i64,
        absolute_limit_seconds: i64,
        grace_seconds: i64,
    ) -> Result<Option<PresentedRefreshToken>, sqlx::Error> {
        sqlx::query_as!(
            PresentedRefreshToken,
            r#"SELECT session_tokens.id AS token_id,
                      sessions.id AS session_id,
                      sessions.user_id,
                      (sessions.revoked_at IS NULL
                        AND sessions.last_seen_at > now() - make_interval(secs => $2::float8)
                        AND sessions.created_at > now() - make_interval(secs => $3::float8)) AS "is_session_live!",
                      session_tokens.replaced_at IS NOT NULL AS "is_replaced!",
                      coalesce(session_tokens.replaced_at > now() - make_interval(secs => $4::float8)
                        AND session_tokens.grace_used_at IS NULL, false) AS "is_within_grace!"
               FROM session_tokens
               JOIN sessions ON sessions.id = session_tokens.session_id
               WHERE session_tokens.token_hash = $1
                 AND session_tokens.kind = 'refresh'
                 AND session_tokens.expires_at > now()
               FOR UPDATE OF session_tokens, sessions"#,
            token_hash,
            idle_limit_seconds as f64,
            absolute_limit_seconds as f64,
            grace_seconds as f64,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn mark_token_replaced(
        &self,
        executor: impl PgExecutor<'_>,
        token_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE session_tokens SET replaced_at = now() WHERE id = $1",
            token_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn mark_grace_used(
        &self,
        executor: impl PgExecutor<'_>,
        token_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE session_tokens SET grace_used_at = now() WHERE id = $1",
            token_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn mark_seen(
        &self,
        executor: impl PgExecutor<'_>,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE sessions SET last_seen_at = now() WHERE id = $1",
            session_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn revoke(
        &self,
        executor: impl PgExecutor<'_>,
        session_id: Uuid,
        revoked_reason: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE sessions
             SET revoked_at = now(), revoked_reason = $2
             WHERE id = $1 AND revoked_at IS NULL",
            session_id,
            revoked_reason,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn revoke_all_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        revoked_reason: &str,
    ) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query!(
            "UPDATE sessions
             SET revoked_at = now(), revoked_reason = $2
             WHERE user_id = $1 AND revoked_at IS NULL",
            user_id,
            revoked_reason,
        )
        .execute(executor)
        .await?
        .rows_affected())
    }

    pub async fn revoke_all_for_user_except(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        kept_session_id: Uuid,
        revoked_reason: &str,
    ) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query!(
            "UPDATE sessions
             SET revoked_at = now(), revoked_reason = $3
             WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL",
            user_id,
            kept_session_id,
            revoked_reason,
        )
        .execute(executor)
        .await?
        .rows_affected())
    }

    pub async fn find_active_by_access_token_hash(
        &self,
        executor: impl PgExecutor<'_>,
        token_hash: &[u8],
        idle_limit_seconds: i64,
        absolute_limit_seconds: i64,
    ) -> Result<Option<ActiveSession>, sqlx::Error> {
        sqlx::query_as!(
            ActiveSession,
            r#"SELECT sessions.id AS session_id, sessions.user_id
               FROM session_tokens
               JOIN sessions ON sessions.id = session_tokens.session_id
               WHERE session_tokens.token_hash = $1
                 AND session_tokens.kind = 'access'
                 AND session_tokens.expires_at > now()
                 AND sessions.revoked_at IS NULL
                 AND sessions.last_seen_at > now() - make_interval(secs => $2::float8)
                 AND sessions.created_at > now() - make_interval(secs => $3::float8)"#,
            token_hash,
            idle_limit_seconds as f64,
            absolute_limit_seconds as f64,
        )
        .fetch_optional(executor)
        .await
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::security::opaque_token::hash_opaque_token;
    use crate::features::sessions::entities::session::{
        SESSION_ABSOLUTE_LIMIT_SECONDS, SESSION_IDLE_LIMIT_SECONDS,
    };
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

    async fn seed_session(db_pool: &PgPool) -> (Uuid, Uuid) {
        let user_id = seed_user_id(db_pool).await;
        let session_id = PostgresSessionSource
            .insert_session(
                db_pool,
                user_id,
                Some("180.191.77.129".parse().expect("valid address")),
                Some("DaiBX test"),
            )
            .await
            .expect("inserting the session should succeed");
        (user_id, session_id)
    }

    async fn find_active(db_pool: &PgPool, opaque_token: &str) -> Option<ActiveSession> {
        PostgresSessionSource
            .find_active_by_access_token_hash(
                db_pool,
                &hash_opaque_token(opaque_token),
                SESSION_IDLE_LIMIT_SECONDS,
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await
            .expect("lookup should succeed")
    }

    #[sqlx::test]
    async fn finds_the_live_session_behind_an_access_token(db_pool: PgPool) {
        let (user_id, session_id) = seed_session(&db_pool).await;
        PostgresSessionSource
            .insert_token(
                &db_pool,
                session_id,
                SessionTokenKind::Access,
                &hash_opaque_token("access-token"),
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await
            .expect("inserting the token should succeed");

        assert_eq!(
            find_active(&db_pool, "access-token").await,
            Some(ActiveSession {
                session_id,
                user_id
            })
        );
        assert_eq!(find_active(&db_pool, "unknown-token").await, None);
    }

    #[sqlx::test]
    async fn never_treats_a_refresh_token_as_an_access_token(db_pool: PgPool) {
        let (_, session_id) = seed_session(&db_pool).await;
        PostgresSessionSource
            .insert_token(
                &db_pool,
                session_id,
                SessionTokenKind::Refresh,
                &hash_opaque_token("refresh-token"),
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await
            .expect("inserting the token should succeed");

        assert_eq!(find_active(&db_pool, "refresh-token").await, None);
    }
}
