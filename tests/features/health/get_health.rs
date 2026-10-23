use axum::http::StatusCode;
use sqlx::PgPool;

use crate::support::{build_test_app, get, response_json};

#[sqlx::test]
async fn get_health_returns_200_when_the_database_answers(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/health").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "ok");
}

#[sqlx::test]
async fn get_health_returns_500_when_the_database_is_unreachable(db_pool: PgPool) {
    db_pool.close().await;

    let response = get(build_test_app(db_pool), "/health").await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response_json(response).await["error"],
        "internal server error"
    );
}
