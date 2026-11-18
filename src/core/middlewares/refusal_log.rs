use std::sync::{Arc, OnceLock};

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

const UNMATCHED_ROUTE: &str = "unmatched";

#[derive(Debug, Clone, Default)]
pub struct RequestAccount(Arc<OnceLock<Uuid>>);

impl RequestAccount {
    pub fn record(&self, security_ref: Uuid) {
        let _ = self.0.set(security_ref);
    }

    fn security_ref(&self) -> Option<Uuid> {
        self.0.get().copied()
    }
}

pub async fn log_refused_request(mut request: Request, next: Next) -> Response {
    let matched_route = request.extensions().get::<MatchedPath>().cloned();
    let request_method = request.method().clone();
    let request_account = RequestAccount::default();
    request.extensions_mut().insert(request_account.clone());

    let response = next.run(request).await;

    if response.status().is_client_error() {
        tracing::info!(
            method = %request_method,
            route = matched_route
                .as_ref()
                .map_or(UNMATCHED_ROUTE, MatchedPath::as_str),
            status = response.status().as_u16(),
            security_ref = request_account.security_ref().map(tracing::field::display),
            "request refused"
        );
    }
    response
}
