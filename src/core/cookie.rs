use axum::http::{HeaderMap, HeaderValue, header};

pub fn build_host_cookie(
    cookie_name: &str,
    cookie_value: &str,
    max_age_seconds: i64,
) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{cookie_name}={cookie_value}; Path=/; Max-Age={max_age_seconds}; HttpOnly; Secure; SameSite=Lax"
    ))
    .expect("cookie names and base64url values are valid header text")
}

pub fn build_expired_host_cookie(cookie_name: &str) -> HeaderValue {
    build_host_cookie(cookie_name, "", 0)
}

pub fn read_cookie<'a>(request_headers: &'a HeaderMap, cookie_name: &str) -> Option<&'a str> {
    request_headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header_value| header_value.to_str().ok())
        .flat_map(|cookie_header| cookie_header.split(';'))
        .filter_map(|cookie_pair| cookie_pair.trim().split_once('='))
        .find(|(pair_name, pair_value)| *pair_name == cookie_name && !pair_value.is_empty())
        .map(|(_, pair_value)| pair_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_an_expired_cookie_that_clears_the_value() {
        assert_eq!(
            build_expired_host_cookie("__Host-daibx_challenge"),
            "__Host-daibx_challenge=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax"
        );
    }

    #[test]
    fn reads_a_named_cookie_from_any_cookie_header() {
        let mut request_headers = HeaderMap::new();
        request_headers.append(header::COOKIE, HeaderValue::from_static("theme=dark"));
        request_headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Host-daibx_access=abc; __Host-daibx_refresh=def"),
        );

        assert_eq!(
            read_cookie(&request_headers, "__Host-daibx_refresh"),
            Some("def")
        );
        assert_eq!(
            read_cookie(&request_headers, "__Host-daibx_access"),
            Some("abc")
        );
        assert_eq!(read_cookie(&request_headers, "__Host-daibx_device"), None);
    }

    #[test]
    fn ignores_an_empty_cookie_value() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-daibx_access="),
        );

        assert_eq!(read_cookie(&request_headers, "__Host-daibx_access"), None);
    }

    #[test]
    fn builds_a_host_only_secure_cookie() {
        let cookie_header = build_host_cookie("__Host-daibx_challenge", "abc_DEF-123", 300);

        assert_eq!(
            cookie_header,
            "__Host-daibx_challenge=abc_DEF-123; Path=/; Max-Age=300; HttpOnly; Secure; SameSite=Lax"
        );
    }
}
