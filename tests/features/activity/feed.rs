use axum::http::StatusCode;
use sqlx::PgPool;

use crate::support::{SignedInPerson, build_test_app, get_with_cookies, response_json};

pub async fn get_activity(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    query_text: &str,
) -> axum::response::Response {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/activity{query_text}"),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn list_entries(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> Vec<serde_json::Value> {
    let response = get_activity(db_pool, signed_in_person, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["entries"]
        .as_array()
        .expect("entries should be a list")
        .clone()
}

pub fn entry_kinds(activity_entries: &[serde_json::Value]) -> Vec<&str> {
    activity_entries
        .iter()
        .map(|activity_entry| activity_entry["kind"].as_str().expect("a kind"))
        .collect()
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use daibx_api::core::utc_instant::UtcInstant;
    use serde_json::json;
    use sqlx::PgPool;

    use super::*;
    use crate::support::{delete_with_cookies, get, sign_up_with_authenticator};
    use crate::tasks::completion::set_task_done;
    use crate::tracking::start::add_top_level_task;

    async fn tick_three_tasks(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> Vec<String> {
        let mut task_ids = Vec::with_capacity(3);
        for task_name in ["First", "Second", "Third"] {
            let (task_id, _) = add_top_level_task(db_pool, signed_in_person, task_name).await;
            set_task_done(db_pool, signed_in_person, &task_id, true).await;
            task_ids.push(task_id);
        }
        task_ids
    }

    fn entry_titles(activity_entries: &[serde_json::Value]) -> Vec<&str> {
        activity_entries
            .iter()
            .map(|activity_entry| activity_entry["title"].as_str().expect("a title"))
            .collect()
    }

    #[sqlx::test]
    async fn get_activity_returns_200_newest_first_with_no_next_cursor(db_pool: PgPool) {
        let signed_in_person = sign_up_with_authenticator(&db_pool).await;
        tick_three_tasks(&db_pool, &signed_in_person).await;

        let response = get_activity(&db_pool, &signed_in_person, "").await;

        assert_eq!(response.status(), StatusCode::OK);
        let activity_page = response_json(response).await;
        assert_eq!(activity_page["nextCursor"], serde_json::Value::Null);
        let activity_entries = activity_page["entries"].as_array().expect("entries");
        assert_eq!(
            entry_titles(activity_entries),
            vec!["Third", "Second", "First"]
        );
        assert!(
            UtcInstant::parse(activity_entries[0]["occurredAt"].as_str().expect("a time"))
                .is_some()
        );
    }

    #[sqlx::test]
    async fn get_activity_pages_with_the_next_cursor(db_pool: PgPool) {
        let signed_in_person = sign_up_with_authenticator(&db_pool).await;
        tick_three_tasks(&db_pool, &signed_in_person).await;

        let first_page =
            response_json(get_activity(&db_pool, &signed_in_person, "?limit=2").await).await;
        let next_cursor = first_page["nextCursor"].as_str().expect("a next cursor");
        let second_page = response_json(
            get_activity(
                &db_pool,
                &signed_in_person,
                &format!("?limit=2&cursor={next_cursor}"),
            )
            .await,
        )
        .await;

        assert_eq!(
            entry_titles(first_page["entries"].as_array().expect("entries")),
            vec!["Third", "Second"]
        );
        assert_eq!(
            entry_titles(second_page["entries"].as_array().expect("entries")),
            vec!["First"]
        );
        assert_eq!(second_page["nextCursor"], serde_json::Value::Null);
    }

    #[sqlx::test]
    async fn get_activity_keeps_only_entries_since_the_given_instant(db_pool: PgPool) {
        let signed_in_person = sign_up_with_authenticator(&db_pool).await;
        tick_three_tasks(&db_pool, &signed_in_person).await;
        let future_since = UtcInstant::now().plus_minutes(5).to_iso_text();

        let since_page = response_json(
            get_activity(
                &db_pool,
                &signed_in_person,
                &format!("?since={future_since}"),
            )
            .await,
        )
        .await;

        assert_eq!(since_page["entries"], json!([]));
    }

    #[sqlx::test]
    async fn get_activity_refuses_a_bad_limit_cursor_or_since_with_422(db_pool: PgPool) {
        let signed_in_person = sign_up_with_authenticator(&db_pool).await;

        for query_text in [
            "?limit=0",
            "?limit=101",
            "?cursor=not-a-cursor",
            "?since=yesterday",
        ] {
            let response = get_activity(&db_pool, &signed_in_person, query_text).await;
            assert_eq!(
                response.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{query_text}"
            );
        }
    }

    #[sqlx::test]
    async fn get_activity_refuses_a_request_without_a_session_with_401(db_pool: PgPool) {
        let response = get(build_test_app(db_pool), "/activity").await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn get_activity_never_shows_another_persons_entries(db_pool: PgPool) {
        let signed_in_person = sign_up_with_authenticator(&db_pool).await;
        let other_person = sign_up_with_authenticator(&db_pool).await;
        tick_three_tasks(&db_pool, &other_person).await;

        assert!(list_entries(&db_pool, &signed_in_person).await.is_empty());
    }

    #[sqlx::test]
    async fn a_deleted_task_keeps_its_entry_without_the_link(db_pool: PgPool) {
        let signed_in_person = sign_up_with_authenticator(&db_pool).await;
        let task_ids = tick_three_tasks(&db_pool, &signed_in_person).await;

        let response = delete_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/tasks/{}", task_ids[0]),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let activity_entries = list_entries(&db_pool, &signed_in_person).await;
        let first_entry = activity_entries
            .iter()
            .find(|activity_entry| activity_entry["title"] == "First")
            .expect("the deleted task's entry should stay");
        assert_eq!(first_entry["taskId"], serde_json::Value::Null);
        assert_eq!(first_entry["contextName"], "Today");
    }
}
