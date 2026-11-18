use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;

const NEVER_STORE: HeaderValue = HeaderValue::from_static("no-store");
const NO_TYPE_SNIFFING: HeaderValue = HeaderValue::from_static("nosniff");

pub async fn add_response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let response_headers = response.headers_mut();
    response_headers.insert(header::CACHE_CONTROL, NEVER_STORE);
    response_headers.insert(header::X_CONTENT_TYPE_OPTIONS, NO_TYPE_SNIFFING);
    response
}
