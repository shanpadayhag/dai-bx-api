use std::env;
use std::time::Duration;

use daibx_api::core::utc_instant::UtcInstant;
use daibx_api::features::card_schedules::actions::run_due_schedules::RunDueSchedules;
use lambda_http::lambda_runtime::{self, LambdaEvent, service_fn};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

const DATABASE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

async fn run_due_schedules(db_pool: &PgPool) -> Result<Value, lambda_runtime::Error> {
    let due_run_tally = RunDueSchedules::new(db_pool)
        .execute(UtcInstant::now())
        .await?;
    tracing::info!(
        ran_count = due_run_tally.ran_count,
        failed_count = due_run_tally.failed_count,
        "due schedules ran"
    );
    Ok(json!({
        "ranCount": due_run_tally.ran_count,
        "failedCount": due_run_tally.failed_count,
    }))
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(DATABASE_ACQUIRE_TIMEOUT)
        .connect(&database_url)
        .await
        .expect("the database must be reachable at startup");
    let db_pool = &db_pool;
    lambda_runtime::run(service_fn(|_: LambdaEvent<Value>| {
        run_due_schedules(db_pool)
    }))
    .await
}
