use std::time::Duration;

use reqwest::Client;

pub const HIBP_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

pub struct HibpPwnedPasswordsSource<'a> {
    http_client: &'a Client,
    api_base_url: &'a str,
}

impl<'a> HibpPwnedPasswordsSource<'a> {
    pub fn new(http_client: &'a Client, api_base_url: &'a str) -> Self {
        Self {
            http_client,
            api_base_url,
        }
    }

    pub async fn count_breaches(&self, password_sha1_hex: &str) -> Result<u64, reqwest::Error> {
        let (hash_prefix, hash_suffix) = password_sha1_hex.split_at(HASH_PREFIX_LENGTH);
        let range_body = self
            .http_client
            .get(format!(
                "{}/range/{hash_prefix}",
                self.api_base_url.trim_end_matches('/')
            ))
            .header("Add-Padding", "true")
            .timeout(HIBP_REQUEST_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(find_suffix_count(&range_body, hash_suffix))
    }
}

const HASH_PREFIX_LENGTH: usize = 5;

fn find_suffix_count(range_body: &str, hash_suffix: &str) -> u64 {
    range_body
        .lines()
        .filter_map(|range_line| range_line.trim().split_once(':'))
        .find(|(listed_suffix, _)| listed_suffix.eq_ignore_ascii_case(hash_suffix))
        .and_then(|(_, listed_count)| listed_count.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::core::http_client::build_http_client;

    const PASSWORD_SHA1_HEX: &str = "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8";

    async fn serve_range(range_body: &str) -> MockServer {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/range/5BAA6"))
            .and(header("add-padding", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_string(range_body))
            .expect(1)
            .mount(&mock_server)
            .await;
        mock_server
    }

    #[tokio::test]
    async fn sends_only_the_five_character_prefix_and_counts_the_matching_suffix() {
        let mock_server = serve_range(
            "0018A45C4D1DEF81644B54AB7F969B88D65:3\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:52256179\r\n",
        )
        .await;
        let http_client = build_http_client();

        let breach_count = HibpPwnedPasswordsSource::new(&http_client, &mock_server.uri())
            .count_breaches(PASSWORD_SHA1_HEX)
            .await
            .expect("range lookup should succeed");

        assert_eq!(breach_count, 52_256_179);
    }

    #[tokio::test]
    async fn treats_a_padding_entry_with_count_zero_as_not_breached() {
        let mock_server = serve_range("1E4C9B93F3F0682250B6CF8331B7EE68FD8:0\r\n").await;
        let http_client = build_http_client();

        let breach_count = HibpPwnedPasswordsSource::new(&http_client, &mock_server.uri())
            .count_breaches(PASSWORD_SHA1_HEX)
            .await
            .expect("range lookup should succeed");

        assert_eq!(breach_count, 0);
    }

    #[tokio::test]
    async fn counts_zero_when_the_suffix_is_absent() {
        let mock_server = serve_range("0018A45C4D1DEF81644B54AB7F969B88D65:3\n").await;
        let http_client = build_http_client();

        let breach_count = HibpPwnedPasswordsSource::new(&http_client, &mock_server.uri())
            .count_breaches(PASSWORD_SHA1_HEX)
            .await
            .expect("range lookup should succeed");

        assert_eq!(breach_count, 0);
    }

    #[tokio::test]
    async fn fails_when_hibp_answers_with_a_server_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;
        let http_client = build_http_client();

        let lookup_result = HibpPwnedPasswordsSource::new(&http_client, &mock_server.uri())
            .count_breaches(PASSWORD_SHA1_HEX)
            .await;

        assert!(lookup_result.is_err());
    }

    #[tokio::test]
    async fn gives_up_when_hibp_takes_longer_than_the_timeout() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("")
                    .set_delay(HIBP_REQUEST_TIMEOUT + Duration::from_millis(500)),
            )
            .mount(&mock_server)
            .await;
        let http_client = build_http_client();

        let lookup_result = HibpPwnedPasswordsSource::new(&http_client, &mock_server.uri())
            .count_breaches(PASSWORD_SHA1_HEX)
            .await;

        assert!(lookup_result.is_err_and(|lookup_error| lookup_error.is_timeout()));
    }
}
