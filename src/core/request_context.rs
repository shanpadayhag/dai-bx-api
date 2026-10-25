use std::convert::Infallible;
use std::net::IpAddr;

use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;

use crate::core::middlewares::client_ip::ClientIp;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestContext {
    pub client_ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for RequestContext {
    type Rejection = Infallible;

    async fn from_request_parts(request_parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self {
            client_ip: request_parts
                .extensions
                .get::<ClientIp>()
                .and_then(|client_ip| client_ip.0),
            user_agent: request_parts
                .headers
                .get(header::USER_AGENT)
                .and_then(|header_value| header_value.to_str().ok())
                .map(str::to_owned),
        })
    }
}
