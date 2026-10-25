use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::core::config::app_config::AppConfig;
use crate::core::error::AppError;
use crate::core::state::AppState;

const SEC_FETCH_SITE: &str = "sec-fetch-site";
const SAME_ORIGIN: &str = "same-origin";
const JSON_MEDIA_TYPE: &str = "application/json";

pub async fn require_same_origin_json(
    State(app_state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    check_state_changing_request(request.method(), request.headers(), &app_state.app_config)?;
    Ok(next.run(request).await)
}

fn check_state_changing_request(
    request_method: &Method,
    request_headers: &HeaderMap,
    app_config: &AppConfig,
) -> Result<(), AppError> {
    if is_safe_method(request_method) {
        return Ok(());
    }
    if !is_from_the_app(request_headers, app_config) {
        return Err(AppError::Forbidden);
    }
    if !is_json_request(request_headers) {
        return Err(AppError::UnsupportedMediaType);
    }
    Ok(())
}

fn is_safe_method(request_method: &Method) -> bool {
    matches!(
        *request_method,
        Method::GET | Method::HEAD | Method::OPTIONS
    )
}

fn is_from_the_app(request_headers: &HeaderMap, app_config: &AppConfig) -> bool {
    match read_header(request_headers, SEC_FETCH_SITE) {
        Some(fetch_site) => fetch_site == SAME_ORIGIN,
        None => read_header(request_headers, header::ORIGIN.as_str())
            .is_some_and(|request_origin| app_config.is_allowed_app_origin(request_origin)),
    }
}

fn is_json_request(request_headers: &HeaderMap) -> bool {
    read_header(request_headers, header::CONTENT_TYPE.as_str()).is_some_and(|content_type| {
        content_type
            .split(';')
            .next()
            .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case(JSON_MEDIA_TYPE))
    })
}

fn read_header<'a>(request_headers: &'a HeaderMap, header_name: &str) -> Option<&'a str> {
    request_headers
        .get(header_name)
        .and_then(|header_value| header_value.to_str().ok())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;
    use crate::core::config::app_config::AllowedOrigin;

    fn app_config() -> AppConfig {
        AppConfig {
            database_url: String::from("postgres://unused"),
            allowed_app_origins: vec![
                AllowedOrigin::parse("http://localhost:3000").expect("origin should parse"),
            ],
            totp_keyring: crate::core::security::totp_secret_cipher::TotpKeyring::parse(
                "v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
                "v1",
            )
            .expect("test TOTP keyring should parse"),
            hibp_api_base_url: String::from("http://unused"),
            app_origin: String::from("http://localhost:3000"),
            smtp_settings: crate::core::config::smtp_settings::SmtpSettings {
                host: String::from("127.0.0.1"),
                port: 1025,
                security: crate::core::config::smtp_settings::SmtpSecurity::Plain,
                credentials: None,
                mail_from: "DaiBX <no-reply@daibx.test>"
                    .parse()
                    .expect("test sender should parse"),
            },
            open_signups: false,
            terms_version: String::from("test-terms-1"),
        }
    }

    fn headers(header_pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        header_pairs
            .iter()
            .map(|(header_name, header_value)| {
                (
                    header::HeaderName::from_static(header_name),
                    HeaderValue::from_static(header_value),
                )
            })
            .collect()
    }

    #[test]
    fn lets_safe_methods_through_without_any_headers() {
        for safe_method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(
                check_state_changing_request(&safe_method, &HeaderMap::new(), &app_config())
                    .is_ok()
            );
        }
    }

    #[test]
    fn trusts_fetch_metadata_over_the_origin_header() {
        let request_headers = headers(&[
            ("sec-fetch-site", "cross-site"),
            ("origin", "http://localhost:3000"),
            ("content-type", "application/json"),
        ]);

        let check_result =
            check_state_changing_request(&Method::POST, &request_headers, &app_config());

        assert!(matches!(check_result, Err(AppError::Forbidden)));
    }

    #[test]
    fn checks_the_origin_before_the_content_type() {
        let request_headers = headers(&[
            ("origin", "https://evil.example"),
            ("content-type", "text/plain"),
        ]);

        let check_result =
            check_state_changing_request(&Method::DELETE, &request_headers, &app_config());

        assert!(matches!(check_result, Err(AppError::Forbidden)));
    }

    #[test]
    fn accepts_a_json_media_type_in_any_case_with_parameters() {
        let request_headers = headers(&[
            ("sec-fetch-site", "same-origin"),
            ("content-type", "Application/JSON; charset=UTF-8"),
        ]);

        assert!(
            check_state_changing_request(&Method::PATCH, &request_headers, &app_config()).is_ok()
        );
    }

    #[test]
    fn rejects_a_json_lookalike_media_type() {
        let request_headers = headers(&[
            ("sec-fetch-site", "same-origin"),
            ("content-type", "application/jsonp"),
        ]);

        let check_result =
            check_state_changing_request(&Method::POST, &request_headers, &app_config());

        assert!(matches!(check_result, Err(AppError::UnsupportedMediaType)));
    }
}
