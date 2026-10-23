use sqlx::PgPool;

use crate::features::health::data_sources::postgres::health_source::PostgresHealthSource;
use crate::features::health::error::HealthError;

pub struct CheckHealth<'a> {
    postgres_health_source: PostgresHealthSource<'a>,
}

impl<'a> CheckHealth<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            postgres_health_source: PostgresHealthSource::new(db_pool),
        }
    }

    pub async fn execute(&self) -> Result<(), HealthError> {
        Ok(self.postgres_health_source.ping().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn reports_healthy_when_the_database_answers(db_pool: PgPool) {
        CheckHealth::new(&db_pool)
            .execute()
            .await
            .expect("a reachable database should be healthy");
    }

    #[sqlx::test]
    async fn reports_a_database_error_when_the_database_is_unreachable(db_pool: PgPool) {
        db_pool.close().await;

        let health_result = CheckHealth::new(&db_pool).execute().await;

        assert!(matches!(health_result, Err(HealthError::Database(_))));
    }
}
