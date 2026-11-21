use std::time::Duration;

use data_encoding::BASE64URL_NOPAD;
use reqwest::{Client, StatusCode};
use web_push_native::p256::PublicKey;
use web_push_native::{Auth, WebPushBuilder};

use crate::core::config::vapid_settings::VapidSettings;
use crate::features::notifications::entities::push_device_keys::PushDeviceKeys;
use crate::features::notifications::entities::push_message::PushMessage;

const PUSH_SEND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Delivered,
    Gone,
    Refused(u16),
}

#[derive(Debug, thiserror::Error)]
pub enum WebPushError {
    #[error("the device keys could not be used")]
    DeviceKeys,
    #[error("the push request could not be built")]
    Build,
    #[error("the push service could not be reached")]
    Transport(#[source] reqwest::Error),
}

pub struct WebPushSource<'a> {
    http_client: &'a Client,
    vapid_settings: &'a VapidSettings,
}

impl<'a> WebPushSource<'a> {
    pub fn new(http_client: &'a Client, vapid_settings: &'a VapidSettings) -> Self {
        Self {
            http_client,
            vapid_settings,
        }
    }

    pub async fn send(
        &self,
        endpoint: &str,
        device_keys: &PushDeviceKeys,
        push_message: &PushMessage,
    ) -> Result<PushOutcome, WebPushError> {
        let push_request = self.build_push_request(endpoint, device_keys, push_message)?;
        let (request_parts, encrypted_body) = push_request.into_parts();
        let mut outgoing_request = self
            .http_client
            .post(request_parts.uri.to_string())
            .headers(request_parts.headers)
            .header("Urgency", push_message.urgency.as_str())
            .timeout(PUSH_SEND_TIMEOUT)
            .body(encrypted_body);
        if let Some(topic) = &push_message.topic {
            outgoing_request = outgoing_request.header("Topic", topic);
        }
        let push_response = outgoing_request
            .send()
            .await
            .map_err(WebPushError::Transport)?;
        Ok(match push_response.status() {
            success_status if success_status.is_success() => PushOutcome::Delivered,
            StatusCode::NOT_FOUND | StatusCode::GONE => PushOutcome::Gone,
            refused_status => PushOutcome::Refused(refused_status.as_u16()),
        })
    }

    fn build_push_request(
        &self,
        endpoint: &str,
        device_keys: &PushDeviceKeys,
        push_message: &PushMessage,
    ) -> Result<http::Request<Vec<u8>>, WebPushError> {
        let device_public_key = BASE64URL_NOPAD
            .decode(device_keys.p256dh.as_bytes())
            .ok()
            .and_then(|public_key_bytes| PublicKey::from_sec1_bytes(&public_key_bytes).ok())
            .ok_or(WebPushError::DeviceKeys)?;
        let device_auth_secret = BASE64URL_NOPAD
            .decode(device_keys.auth_secret.as_bytes())
            .ok()
            .and_then(|auth_secret_bytes| <[u8; 16]>::try_from(auth_secret_bytes).ok())
            .map(Auth::from)
            .ok_or(WebPushError::DeviceKeys)?;
        let endpoint_uri = endpoint.parse().map_err(|_| WebPushError::Build)?;
        WebPushBuilder::new(endpoint_uri, device_public_key, device_auth_secret)
            .with_valid_duration(push_message.time_to_live)
            .with_vapid(&self.vapid_settings.key_pair, &self.vapid_settings.subject)
            .build(push_message.to_payload_bytes())
            .map_err(|_| WebPushError::Build)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use web_push_native::p256::SecretKey;
    use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::core::http_client::build_http_client;

    const TEST_VAPID_PRIVATE_KEY: &str = "RS0WdYWWo1HajXg3NZR1olzCf31i-ZBGDkFyCs7j1jw";
    const TEST_DEVICE_SECRET_BYTES: [u8; 32] = [7; 32];
    const TEST_AUTH_SECRET_BYTES: [u8; 16] = [3; 16];

    fn test_vapid_settings() -> VapidSettings {
        VapidSettings::from_private_key(
            TEST_VAPID_PRIVATE_KEY,
            String::from("https://dai-bx.vercel.app"),
        )
        .expect("test VAPID key should parse")
    }

    fn test_device_secret() -> SecretKey {
        SecretKey::from_slice(&TEST_DEVICE_SECRET_BYTES).expect("test device key should parse")
    }

    fn test_device_keys() -> PushDeviceKeys {
        let device_public_point = test_device_secret().public_key().to_encoded_point(false);
        PushDeviceKeys {
            p256dh: BASE64URL_NOPAD.encode(device_public_point.as_bytes()),
            auth_secret: BASE64URL_NOPAD.encode(&TEST_AUTH_SECRET_BYTES),
        }
    }

    async fn push_service_answering(response_status: u16) -> MockServer {
        let push_service = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/push/device"))
            .respond_with(ResponseTemplate::new(response_status))
            .mount(&push_service)
            .await;
        push_service
    }

    async fn send_check_in(push_service: &MockServer) -> Result<PushOutcome, WebPushError> {
        let http_client = build_http_client();
        let vapid_settings = test_vapid_settings();
        WebPushSource::new(&http_client, &vapid_settings)
            .send(
                &format!("{}/push/device", push_service.uri()),
                &test_device_keys(),
                &PushMessage::check_in(uuid::Uuid::nil(), "Write the spec", "DaiBX v3", 45),
            )
            .await
    }

    #[tokio::test]
    async fn sends_an_encrypted_signed_message_the_device_can_read() {
        let push_service = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("content-encoding", "aes128gcm"))
            .and(header("ttl", "600"))
            .and(header("urgency", "high"))
            .and(header_exists("topic"))
            .and(header_exists("authorization"))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&push_service)
            .await;

        let push_outcome = send_check_in(&push_service)
            .await
            .expect("the send should reach the push service");

        assert_eq!(push_outcome, PushOutcome::Delivered);
        let received_requests = push_service
            .received_requests()
            .await
            .expect("requests should be recorded");
        let authorization_header = received_requests[0]
            .headers
            .get("authorization")
            .and_then(|header_value| header_value.to_str().ok())
            .expect("the request should carry an authorization header");
        assert!(authorization_header.starts_with("vapid t="));
        assert!(authorization_header.contains(&format!(
            "k={}",
            test_vapid_settings().application_server_key
        )));
        let decrypted_payload = web_push_native::decrypt(
            received_requests[0].body.clone(),
            &test_device_secret(),
            &Auth::from(TEST_AUTH_SECRET_BYTES),
        )
        .expect("the device should decrypt the payload");
        let payload_json: Value =
            serde_json::from_slice(&decrypted_payload).expect("payload should be JSON");
        assert_eq!(payload_json["title"], "Still on Write the spec?");
    }

    #[tokio::test]
    async fn reports_a_device_the_push_service_no_longer_knows_as_gone() {
        for gone_status in [404, 410] {
            let push_service = push_service_answering(gone_status).await;

            let push_outcome = send_check_in(&push_service)
                .await
                .expect("the push service should answer");

            assert_eq!(push_outcome, PushOutcome::Gone);
        }
    }

    #[tokio::test]
    async fn reports_any_other_refusal_with_its_status() {
        for refused_status in [400, 413, 429, 500] {
            let push_service = push_service_answering(refused_status).await;

            let push_outcome = send_check_in(&push_service)
                .await
                .expect("the push service should answer");

            assert_eq!(push_outcome, PushOutcome::Refused(refused_status));
        }
    }

    #[tokio::test]
    async fn gives_up_on_a_push_service_that_does_not_answer_in_time() {
        let push_service = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_secs(6)))
            .mount(&push_service)
            .await;

        let push_result = send_check_in(&push_service).await;

        assert!(matches!(push_result, Err(WebPushError::Transport(_))));
    }
}
