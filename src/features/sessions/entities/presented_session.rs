use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use zeroize::Zeroizing;

use crate::core::cookie::read_cookie;
use crate::core::security::opaque_token::{TokenHash, hash_opaque_token};
use crate::features::sessions::entities::session::{ACCESS_COOKIE_NAME, REFRESH_COOKIE_NAME};

#[derive(Default)]
pub struct PresentedSession {
    pub access_token: Option<Zeroizing<String>>,
    pub refresh_token: Option<Zeroizing<String>>,
}

impl PresentedSession {
    pub fn token_hashes(&self) -> Vec<TokenHash> {
        [&self.access_token, &self.refresh_token]
            .into_iter()
            .flatten()
            .map(|presented_token| hash_opaque_token(presented_token))
            .collect()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for PresentedSession {
    type Rejection = Infallible;

    async fn from_request_parts(request_parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let read_presented_token = |cookie_name: &str| {
            read_cookie(&request_parts.headers, cookie_name)
                .map(|cookie_value| Zeroizing::new(cookie_value.to_owned()))
        };
        Ok(Self {
            access_token: read_presented_token(ACCESS_COOKIE_NAME),
            refresh_token: read_presented_token(REFRESH_COOKIE_NAME),
        })
    }
}
