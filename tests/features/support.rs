use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, Response};
use daibx_api::app;
use daibx_api::core::config::app_config::{AllowedOrigin, AppConfig};
use daibx_api::core::state::AppState;
use sqlx::PgPool;
use tower::ServiceExt;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

pub const LISTED_APP_ORIGIN: &str = "http://localhost:3000";
pub const PREVIEW_APP_ORIGIN: &str = "https://dai-bx-web-abc123-shans-projects-a71057cb.vercel.app";

pub fn build_test_app(db_pool: PgPool) -> Router {
    let app_config = AppConfig {
        database_url: String::from("postgres://unused-in-tests"),
        allowed_app_origins: vec![
            AllowedOrigin::parse(LISTED_APP_ORIGIN).expect("listed origin should parse"),
            AllowedOrigin::parse("https://dai-bx-web-*-shans-projects-a71057cb.vercel.app")
                .expect("preview origin pattern should parse"),
        ],
    };
    app(AppState {
        db_pool,
        app_config: Arc::new(app_config),
    })
}

pub async fn send(test_app: Router, request: Request<Body>) -> Response<Body> {
    test_app
        .oneshot(request)
        .await
        .expect("router should always answer")
}

pub async fn get(test_app: Router, request_path: &str) -> Response<Body> {
    let request = Request::get(request_path)
        .body(Body::empty())
        .expect("test request should build");
    send(test_app, request).await
}

pub async fn response_json(response: Response<Body>) -> serde_json::Value {
    let response_bytes = to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&response_bytes).expect("response body should be JSON")
}
