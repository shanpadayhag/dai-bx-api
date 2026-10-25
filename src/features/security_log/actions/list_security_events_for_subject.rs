use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::security_log::data_sources::postgres::security_event_source::PostgresSecurityEventSource;
use crate::features::security_log::entities::security_event::SecurityEvent;
use crate::features::security_log::error::SecurityLogError;

pub struct ListSecurityEventsForSubject {
    postgres_security_event_source: PostgresSecurityEventSource,
}

impl Default for ListSecurityEventsForSubject {
    fn default() -> Self {
        Self::new()
    }
}

impl ListSecurityEventsForSubject {
    pub fn new() -> Self {
        Self {
            postgres_security_event_source: PostgresSecurityEventSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        subject_ref: Uuid,
    ) -> Result<Vec<SecurityEvent>, SecurityLogError> {
        Ok(self
            .postgres_security_event_source
            .find_by_subject_ref(executor, subject_ref)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
    use crate::features::security_log::entities::security_event::{
        NewSecurityEvent, SecurityEventDetails, SecurityEventType,
    };

    #[sqlx::test]
    async fn lists_only_the_subjects_events_oldest_first(db_pool: PgPool) {
        let subject_ref = Uuid::now_v7();
        for (event_subject_ref, event_type) in [
            (subject_ref, SecurityEventType::SignUp),
            (Uuid::now_v7(), SecurityEventType::SignInFailed),
            (subject_ref, SecurityEventType::EmailVerified),
        ] {
            RecordSecurityEvent::new()
                .execute(
                    &db_pool,
                    &NewSecurityEvent {
                        event_type,
                        subject_ref: Some(event_subject_ref),
                        client_ip: None,
                        user_agent: None,
                        details: SecurityEventDetails::default(),
                    },
                )
                .await
                .expect("recording should succeed");
        }

        let listed_event_types: Vec<SecurityEventType> = ListSecurityEventsForSubject::new()
            .execute(&db_pool, subject_ref)
            .await
            .expect("listing should succeed")
            .into_iter()
            .map(|security_event| security_event.event_type)
            .collect();

        assert_eq!(
            listed_event_types,
            vec![SecurityEventType::SignUp, SecurityEventType::EmailVerified]
        );
    }
}
