use std::net::IpAddr;

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

const VERCEL_FORWARDED_FOR: &str = "x-vercel-forwarded-for";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub Option<IpAddr>);

pub async fn attach_client_ip(mut request: Request, next: Next) -> Response {
    let client_ip = ClientIp(read_client_ip(request.headers()));
    request.extensions_mut().insert(client_ip);
    next.run(request).await
}

fn read_client_ip(request_headers: &HeaderMap) -> Option<IpAddr> {
    request_headers
        .get(VERCEL_FORWARDED_FOR)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|forwarded_for| forwarded_for.split(',').next())
        .and_then(|first_address| first_address.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::extract::Extension;
    use axum::http::HeaderValue;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    fn headers(header_pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        header_pairs
            .iter()
            .map(|(header_name, header_value)| {
                (
                    axum::http::HeaderName::from_static(header_name),
                    HeaderValue::from_static(header_value),
                )
            })
            .collect()
    }

    #[test]
    fn reads_the_first_address_vercel_forwarded() {
        let request_headers = headers(&[("x-vercel-forwarded-for", "180.191.77.129, 10.0.0.1")]);

        assert_eq!(
            read_client_ip(&request_headers),
            Some("180.191.77.129".parse().expect("valid address"))
        );
    }

    #[test]
    fn reads_an_ipv6_address() {
        let request_headers = headers(&[("x-vercel-forwarded-for", "2001:fd8:c6e1:8200::1")]);

        assert_eq!(
            read_client_ip(&request_headers),
            Some("2001:fd8:c6e1:8200::1".parse().expect("valid address"))
        );
    }

    #[test]
    fn ignores_x_real_ip_and_x_forwarded_for() {
        let request_headers = headers(&[("x-real-ip", "6.6.6.6"), ("x-forwarded-for", "6.6.6.6")]);

        assert_eq!(read_client_ip(&request_headers), None);
    }

    #[test]
    fn ignores_a_value_that_is_not_an_address() {
        let request_headers = headers(&[("x-vercel-forwarded-for", "not-an-address")]);

        assert_eq!(read_client_ip(&request_headers), None);
    }

    #[tokio::test]
    async fn hands_the_client_ip_to_handlers() {
        let echo_router = Router::new()
            .route(
                "/",
                get(|Extension(client_ip): Extension<ClientIp>| async move {
                    client_ip
                        .0
                        .map(|address| address.to_string())
                        .unwrap_or_default()
                }),
            )
            .layer(axum::middleware::from_fn(attach_client_ip));
        let request = Request::get("/")
            .header("x-vercel-forwarded-for", "180.191.77.129")
            .header("x-real-ip", "6.6.6.6")
            .body(Body::empty())
            .expect("test request should build");

        let response = echo_router
            .oneshot(request)
            .await
            .expect("router should answer");
        let response_bytes = to_bytes(response.into_body(), 64)
            .await
            .expect("body should read");

        assert_eq!(&response_bytes[..], b"180.191.77.129");
    }
}
