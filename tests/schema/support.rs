use sqlx::PgPool;
use sqlx::types::Uuid;

pub async fn seed_user(db_pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
         VALUES (gen_random_uuid(), gen_random_uuid(), 'Test Person', $1, '$argon2id$placeholder', 'draft-1', now())
         RETURNING id",
    )
    .bind(email)
    .fetch_one(db_pool)
    .await
    .expect("seed user insert should succeed")
}

pub async fn seed_session(db_pool: &PgPool, user_id: Uuid, age: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO sessions (id, user_id, created_at, last_seen_at)
         VALUES (gen_random_uuid(), $1, now() - $2::interval, now() - $2::interval)
         RETURNING id",
    )
    .bind(user_id)
    .bind(age)
    .fetch_one(db_pool)
    .await
    .expect("seed session insert should succeed")
}

pub async fn count_rows(db_pool: &PgPool, table_name: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table_name}"
    )))
    .fetch_one(db_pool)
    .await
    .expect("row count should succeed")
}

pub fn random_hash() -> Vec<u8> {
    Uuid::new_v4()
        .as_bytes()
        .iter()
        .chain(Uuid::new_v4().as_bytes())
        .copied()
        .collect()
}
