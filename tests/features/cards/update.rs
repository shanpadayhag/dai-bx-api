use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::cards::create::{board_with_cards, card, card_id};
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn patch_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

async fn read_notes(db_pool: &PgPool, signed_in_person: &SignedInPerson, card_id: &str) -> String {
    response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/cards/{card_id}"),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await["notes"]
        .as_str()
        .expect("notes")
        .to_owned()
}

#[sqlx::test]
async fn changes_the_title_notes_and_due_date_with_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Backlog", &["Plan"])]).await;
    let plan_card_id = card_id(&board, "Plan").to_owned();

    let response = patch_card(
        &db_pool,
        &signed_in_person,
        &plan_card_id,
        json!({
            "title": "  Plan the week ",
            "notes": "## Outcome\nA calm week\n",
            "dueDate": "2026-11-05",
            "dueTime": "18:00",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_board = response_json(response).await;
    let updated_card = card(&updated_board, "Plan the week");
    assert_eq!(updated_card["dueDate"], "2026-11-05");
    assert_eq!(updated_card["dueTime"], "18:00");
    assert!(updated_board["cards"][0].get("notes").is_none());
    assert_eq!(
        read_notes(&db_pool, &signed_in_person, &plan_card_id).await,
        "## Outcome\nA calm week\n"
    );
}

#[sqlx::test]
async fn leaves_missing_fields_alone_and_clears_null_ones(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Backlog", &["Plan"])]).await;
    let plan_card_id = card_id(&board, "Plan").to_owned();
    patch_card(
        &db_pool,
        &signed_in_person,
        &plan_card_id,
        json!({ "dueDate": "2026-11-05", "dueTime": "18:00", "notes": "" }),
    )
    .await;

    let time_cleared = response_json(
        patch_card(
            &db_pool,
            &signed_in_person,
            &plan_card_id,
            json!({ "dueTime": null }),
        )
        .await,
    )
    .await;
    let both_cleared = response_json(
        patch_card(
            &db_pool,
            &signed_in_person,
            &plan_card_id,
            json!({ "dueDate": null, "dueTime": null }),
        )
        .await,
    )
    .await;
    let date_only = response_json(
        patch_card(
            &db_pool,
            &signed_in_person,
            &plan_card_id,
            json!({ "dueDate": "2026-12-31" }),
        )
        .await,
    )
    .await;

    assert_eq!(card(&time_cleared, "Plan")["dueDate"], "2026-11-05");
    assert_eq!(card(&time_cleared, "Plan")["dueTime"], json!(null));
    assert_eq!(card(&both_cleared, "Plan")["dueDate"], json!(null));
    assert_eq!(card(&date_only, "Plan")["dueDate"], "2026-12-31");
    assert_eq!(card(&date_only, "Plan")["dueTime"], json!(null));
    assert_eq!(
        read_notes(&db_pool, &signed_in_person, &plan_card_id).await,
        ""
    );
}

#[sqlx::test]
async fn refuses_bad_titles_notes_dates_and_times(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Backlog", &["Plan"])]).await;
    let plan_card_id = card_id(&board, "Plan").to_owned();

    for (rejected_body, expected_error) in [
        (
            json!({ "title": " " }),
            "a card title is 1 to 500 characters",
        ),
        (
            json!({ "title": "a".repeat(501) }),
            "a card title is 1 to 500 characters",
        ),
        (
            json!({ "title": null }),
            "a card title is 1 to 500 characters",
        ),
        (
            json!({ "notes": "a".repeat(20_001) }),
            "notes are at most 20,000 characters",
        ),
        (
            json!({ "notes": null }),
            "notes can't be null; send \"\" to empty them",
        ),
        (json!({ "dueTime": "09:00" }), "a due time needs a due date"),
        (
            json!({ "dueDate": "2026-02-29" }),
            "a due date is a real date like 2026-11-05",
        ),
        (
            json!({ "dueDate": "5 Nov 2026" }),
            "a due date is a real date like 2026-11-05",
        ),
        (
            json!({ "dueDate": "2026-11-05", "dueTime": "24:00" }),
            "a due time is like 09:30, from 00:00 to 23:59",
        ),
        (
            json!({ "dueDate": "2026-11-05", "dueTime": "9:30" }),
            "a due time is like 09:30, from 00:00 to 23:59",
        ),
    ] {
        let response = patch_card(&db_pool, &signed_in_person, &plan_card_id, rejected_body).await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": expected_error })
        );
    }
    let with_due = patch_card(
        &db_pool,
        &signed_in_person,
        &plan_card_id,
        json!({ "dueDate": "2028-02-29", "dueTime": "23:59", "notes": "a".repeat(20_000) }),
    )
    .await;
    assert_eq!(with_due.status(), StatusCode::OK);
    let date_cleared_under_a_time = patch_card(
        &db_pool,
        &signed_in_person,
        &plan_card_id,
        json!({ "dueDate": null }),
    )
    .await;
    assert_eq!(
        date_cleared_under_a_time.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_card(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &owner, &[("Backlog", &["Plan"])]).await;

    let response = patch_card(
        &db_pool,
        &other_person,
        card_id(&board, "Plan"),
        json!({ "title": "Taken" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
