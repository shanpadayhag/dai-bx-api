use sqlx::PgPool;

pub struct PostgresHealthSource<'a> {
    db_pool: &'a PgPool,
}

impl<'a> PostgresHealthSource<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
            .fetch_one(self.db_pool)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn pings_a_reachable_database(db_pool: PgPool) {
        PostgresHealthSource::new(&db_pool)
            .ping()
            .await
            .expect("a reachable database should answer");
    }

    #[sqlx::test]
    async fn fails_to_ping_a_closed_pool(db_pool: PgPool) {
        db_pool.close().await;

        let ping_result = PostgresHealthSource::new(&db_pool).ping().await;

        assert!(ping_result.is_err());
    }
}
