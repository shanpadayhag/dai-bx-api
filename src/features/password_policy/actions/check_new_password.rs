use reqwest::Client;

use crate::features::password_policy::data_sources::hibp::pwned_passwords_source::HibpPwnedPasswordsSource;
use crate::features::password_policy::entities::new_password::{
    NewPassword, PasswordContext, PasswordRejection,
};
use crate::features::password_policy::error::PasswordPolicyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreachCheck {
    Checked,
    Skipped,
}

pub struct CheckNewPassword<'a> {
    hibp_pwned_passwords_source: HibpPwnedPasswordsSource<'a>,
}

impl<'a> CheckNewPassword<'a> {
    pub fn new(http_client: &'a Client, hibp_api_base_url: &'a str) -> Self {
        Self {
            hibp_pwned_passwords_source: HibpPwnedPasswordsSource::new(
                http_client,
                hibp_api_base_url,
            ),
        }
    }

    pub async fn execute(
        &self,
        password: &str,
        password_context: PasswordContext<'_>,
    ) -> Result<BreachCheck, PasswordPolicyError> {
        let new_password = NewPassword::new(password);
        new_password
            .check_local_rules(password_context)
            .map_err(PasswordPolicyError::Rejected)?;
        match self
            .hibp_pwned_passwords_source
            .count_breaches(&new_password.sha1_hex())
            .await
        {
            Ok(0) => Ok(BreachCheck::Checked),
            Ok(_) => Err(PasswordPolicyError::Rejected(PasswordRejection::Breached)),
            Err(hibp_error) => {
                tracing::warn!(error = %hibp_error, "breached-password check skipped");
                Ok(BreachCheck::Skipped)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::core::http_client::build_http_client;
    use crate::features::password_policy::data_sources::hibp::pwned_passwords_source::HIBP_REQUEST_TIMEOUT;

    const CONTEXT: PasswordContext<'static> = PasswordContext {
        person_name: "Shan Padayhag",
        email: "shan.p@example.com",
    };
    const STRONG_PASSWORD: &str = "violet lantern drifts past seven quiet docks";

    async fn hibp_answering(
        response_template: ResponseTemplate,
        expected_call_count: u64,
    ) -> MockServer {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(response_template)
            .expect(expected_call_count)
            .mount(&mock_server)
            .await;
        mock_server
    }

    fn range_body_listing(password: &str, breach_count: u64) -> String {
        let password_sha1_hex = NewPassword::new(password).sha1_hex();
        format!("{}:{breach_count}\r\n", &password_sha1_hex[5..])
    }

    #[tokio::test]
    async fn refuses_a_password_hibp_lists_as_breached() {
        let mock_server = hibp_answering(
            ResponseTemplate::new(200).set_body_string(range_body_listing(STRONG_PASSWORD, 7)),
            1,
        )
        .await;
        let http_client = build_http_client();

        let check_result = CheckNewPassword::new(&http_client, &mock_server.uri())
            .execute(STRONG_PASSWORD, CONTEXT)
            .await;

        assert!(matches!(
            check_result,
            Err(PasswordPolicyError::Rejected(PasswordRejection::Breached))
        ));
    }

    #[tokio::test]
    async fn accepts_a_strong_password_hibp_does_not_list() {
        let mock_server = hibp_answering(
            ResponseTemplate::new(200).set_body_string(range_body_listing(STRONG_PASSWORD, 0)),
            1,
        )
        .await;
        let http_client = build_http_client();

        let check_result = CheckNewPassword::new(&http_client, &mock_server.uri())
            .execute(STRONG_PASSWORD, CONTEXT)
            .await;

        assert_eq!(
            check_result.expect("strong password should pass"),
            BreachCheck::Checked
        );
    }

    #[tokio::test]
    async fn accepts_a_strong_password_on_local_rules_when_hibp_times_out() {
        let mock_server = hibp_answering(
            ResponseTemplate::new(200).set_delay(HIBP_REQUEST_TIMEOUT + Duration::from_millis(500)),
            1,
        )
        .await;
        let http_client = build_http_client();

        let check_result = CheckNewPassword::new(&http_client, &mock_server.uri())
            .execute(STRONG_PASSWORD, CONTEXT)
            .await;

        assert_eq!(
            check_result.expect("strong password should pass"),
            BreachCheck::Skipped
        );
    }

    #[tokio::test]
    async fn still_refuses_a_weak_password_when_hibp_is_down() {
        let mock_server = hibp_answering(ResponseTemplate::new(503), 0).await;
        let http_client = build_http_client();

        let check_result = CheckNewPassword::new(&http_client, &mock_server.uri())
            .execute("passwordpassword", CONTEXT)
            .await;

        assert!(matches!(
            check_result,
            Err(PasswordPolicyError::Rejected(PasswordRejection::Common))
        ));
    }

    #[tokio::test]
    async fn never_calls_hibp_for_a_password_the_local_rules_refuse() {
        let mock_server = hibp_answering(ResponseTemplate::new(200), 0).await;
        let http_client = build_http_client();

        let check_result = CheckNewPassword::new(&http_client, &mock_server.uri())
            .execute("too short", CONTEXT)
            .await;

        assert!(matches!(
            check_result,
            Err(PasswordPolicyError::Rejected(PasswordRejection::TooShort))
        ));
    }
}
