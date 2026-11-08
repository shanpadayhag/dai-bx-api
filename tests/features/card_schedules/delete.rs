use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::card_schedules::create::{
    board_with_review_card, create_schedule, once_at, schedule_id, weekly_review,
};
use crate::card_schedules::reads::calendar_week;
use crate::support::{
    build_test_app, delete_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn deletes_a_schedule_and_its_blocks_ahead(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let blocked_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        crate::card_schedules::create::merged(
            weekly_review(&scheduled_card.to_do_list_id),
            json!({ "block": { "minutes": 60 } }),
        ),
    )
    .await;
    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.doing_list_id, "2030-03-01T09:00"),
    )
    .await;
    let schedule_path = format!("/schedules/{}", schedule_id(&blocked_schedule));

    let their_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &schedule_path,
        &other_person.session_cookies,
    )
    .await;
    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &schedule_path,
        &signed_in_person.session_cookies,
    )
    .await;
    let deleted_again = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &schedule_path,
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(their_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "schedules": [once_schedule] })
    );
    assert_eq!(deleted_again.status(), StatusCode::NOT_FOUND);
    let week = calendar_week(&db_pool, &signed_in_person, "2030-01-07").await;
    assert_eq!(week["events"], json!([]));
    assert_eq!(week["moves"], json!([]));
}
