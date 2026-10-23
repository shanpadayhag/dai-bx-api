use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::PgPool;

use crate::support::{
    LISTED_APP_ORIGIN, PREVIEW_APP_ORIGIN, build_test_app, get, response_json, send,
};

const JSON_CONTENT_TYPE: &str = "application/json";

fn post_to_health(request_headers: &[(&str, &str)]) -> Request<Body> {
    let request_builder = request_headers.iter().fold(
        Request::post("/health"),
        |request_builder, (header_name, header_value)| {
            request_builder.header(*header_name, *header_value)
        },
    );
    request_builder
        .body(Body::from("{}"))
        .expect("test request should build")
}

#[sqlx::test]
async fn rejects_a_cross_site_post_with_403(db_pool: PgPool) {
    let request = post_to_health(&[
        ("sec-fetch-site", "cross-site"),
        ("content-type", JSON_CONTENT_TYPE),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(response).await["error"], "forbidden");
}

#[sqlx::test]
async fn rejects_a_same_site_post_from_another_subdomain_with_403(db_pool: PgPool) {
    let request = post_to_health(&[
        ("sec-fetch-site", "same-site"),
        ("content-type", JSON_CONTENT_TYPE),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn rejects_a_post_from_an_unlisted_origin_without_fetch_metadata_with_403(db_pool: PgPool) {
    let request = post_to_health(&[
        ("origin", "https://evil.example"),
        ("content-type", JSON_CONTENT_TYPE),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn rejects_a_post_with_neither_fetch_metadata_nor_origin_with_403(db_pool: PgPool) {
    let request = post_to_health(&[("content-type", JSON_CONTENT_TYPE)]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn lets_a_same_origin_json_post_through(db_pool: PgPool) {
    let request = post_to_health(&[
        ("sec-fetch-site", "same-origin"),
        ("content-type", JSON_CONTENT_TYPE),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test]
async fn lets_a_post_from_a_listed_origin_without_fetch_metadata_through(db_pool: PgPool) {
    let request = post_to_health(&[
        ("origin", LISTED_APP_ORIGIN),
        ("content-type", "application/json; charset=utf-8"),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test]
async fn lets_a_post_from_a_preview_origin_without_fetch_metadata_through(db_pool: PgPool) {
    let request = post_to_health(&[
        ("origin", PREVIEW_APP_ORIGIN),
        ("content-type", JSON_CONTENT_TYPE),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test]
async fn rejects_a_same_origin_post_that_is_not_json_with_415(db_pool: PgPool) {
    let request = post_to_health(&[
        ("sec-fetch-site", "same-origin"),
        ("content-type", "application/x-www-form-urlencoded"),
    ]);

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        response_json(response).await["error"],
        "content type must be application/json"
    );
}

#[sqlx::test]
async fn lets_a_get_with_no_headers_through(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/health").await;

    assert_eq!(response.status(), StatusCode::OK);
}
