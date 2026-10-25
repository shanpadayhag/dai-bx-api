use std::time::Duration;

use reqwest::Client;

const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const HTTP_USER_AGENT: &str = "DaiBX-API";

pub fn build_http_client() -> Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .user_agent(HTTP_USER_AGENT)
        .build()
        .expect("the HTTP client must build")
}
