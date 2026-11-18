use sqlx::PgExecutor;

use crate::features::sign_in_alerts::data_sources::postgres::sign_in_alert_source::PostgresSignInAlertSource;
use crate::features::sign_in_alerts::entities::sign_in_alert::NewSignInAlert;
use crate::features::sign_in_alerts::error::SignInAlertError;

#[derive(Default)]
pub struct QueueSignInAlert {
    postgres_sign_in_alert_source: PostgresSignInAlertSource,
}

impl QueueSignInAlert {
    pub fn new() -> Self {
        Self {
            postgres_sign_in_alert_source: PostgresSignInAlertSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        new_alert: &NewSignInAlert<'_>,
    ) -> Result<(), SignInAlertError> {
        Ok(self
            .postgres_sign_in_alert_source
            .insert(executor, new_alert)
            .await?)
    }
}
