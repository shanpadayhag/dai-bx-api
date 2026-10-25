use std::env;
use std::time::Duration;

use serde::Deserialize;

use crate::core::config::smtp_settings::{SmtpSecurity, SmtpSettings};
use crate::core::http_client::build_http_client;

const MAILPIT_LOOKUP_ATTEMPTS: usize = 20;
const MAILPIT_LOOKUP_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Deserialize)]
pub struct DeliveredMessage {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "Text")]
    pub text_body: String,
    #[serde(rename = "HTML")]
    pub html_body: String,
    #[serde(rename = "From")]
    pub sender: DeliveredAddress,
}

#[derive(Debug, Deserialize)]
pub struct DeliveredAddress {
    #[serde(rename = "Address")]
    pub address: String,
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Deserialize)]
struct SearchResult {
    messages: Vec<SearchResultMessage>,
}

#[derive(Deserialize)]
struct SearchResultMessage {
    #[serde(rename = "ID")]
    id: String,
}

pub fn mailpit_smtp_settings() -> SmtpSettings {
    SmtpSettings {
        host: env::var("MAILPIT_SMTP_HOST").unwrap_or_else(|_| String::from("127.0.0.1")),
        port: env::var("MAILPIT_SMTP_PORT")
            .ok()
            .and_then(|smtp_port| smtp_port.parse().ok())
            .unwrap_or(1025),
        security: SmtpSecurity::Plain,
        credentials: None,
        mail_from: "DaiBX <no-reply@daibx.test>"
            .parse()
            .expect("test sender should parse"),
    }
}

pub fn unique_test_recipient() -> String {
    format!("{}@daibx.test", uuid::Uuid::new_v4())
}

pub async fn read_message_sent_to(recipient: &str) -> DeliveredMessage {
    let mailpit_api_url =
        env::var("MAILPIT_API_URL").unwrap_or_else(|_| String::from("http://127.0.0.1:8025"));
    let http_client = build_http_client();
    for _ in 0..MAILPIT_LOOKUP_ATTEMPTS {
        let search_result: SearchResult = http_client
            .get(format!("{mailpit_api_url}/api/v1/search"))
            .query(&[("query", format!("to:\"{recipient}\""))])
            .send()
            .await
            .expect("Mailpit should be running for email tests")
            .json()
            .await
            .expect("Mailpit search should return JSON");
        if let Some(found_message) = search_result.messages.first() {
            return http_client
                .get(format!(
                    "{mailpit_api_url}/api/v1/message/{}",
                    found_message.id
                ))
                .send()
                .await
                .expect("Mailpit message should load")
                .json()
                .await
                .expect("Mailpit message should be JSON");
        }
        tokio::time::sleep(MAILPIT_LOOKUP_INTERVAL).await;
    }
    panic!("no message reached Mailpit for {recipient}");
}
