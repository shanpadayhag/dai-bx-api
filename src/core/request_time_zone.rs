use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use jiff::tz::TimeZone;

use crate::core::recurrence::parse_time_zone;

pub const TIME_ZONE_HEADER_NAME: &str = "x-time-zone";

#[derive(Debug, Clone, Default)]
pub struct RequestTimeZone(pub Option<TimeZone>);

impl RequestTimeZone {
    pub fn calendar_time_zone(&self, stored_time_zone_name: Option<&str>) -> TimeZone {
        stored_time_zone_name
            .and_then(parse_time_zone)
            .or_else(|| self.0.clone())
            .unwrap_or(TimeZone::UTC)
    }
}

fn read_time_zone_header(headers: &HeaderMap) -> Option<TimeZone> {
    headers
        .get(TIME_ZONE_HEADER_NAME)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(parse_time_zone)
}

impl<S: Send + Sync> FromRequestParts<S> for RequestTimeZone {
    type Rejection = Infallible;

    async fn from_request_parts(request_parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(read_time_zone_header(&request_parts.headers)))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers_with_time_zone(time_zone_text: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            TIME_ZONE_HEADER_NAME,
            HeaderValue::from_str(time_zone_text).expect("header value should be valid"),
        );
        headers
    }

    #[test]
    fn reads_an_iana_zone_from_the_header() {
        let read_time_zone = read_time_zone_header(&headers_with_time_zone("Asia/Manila"));

        assert_eq!(
            read_time_zone.and_then(|time_zone| time_zone.iana_name().map(str::to_owned)),
            Some(String::from("Asia/Manila"))
        );
    }

    #[test]
    fn ignores_an_offset_or_junk_in_the_header() {
        for refused_text in ["+08:00", "Not/AZone", ""] {
            assert!(
                read_time_zone_header(&headers_with_time_zone(refused_text)).is_none(),
                "{refused_text}"
            );
        }
    }

    fn zone_name(time_zone: &TimeZone) -> Option<&str> {
        time_zone.iana_name()
    }

    #[test]
    fn the_stored_calendar_zone_wins_over_the_request_zone() {
        let request_time_zone = RequestTimeZone(parse_time_zone("Asia/Manila"));

        let calendar_time_zone = request_time_zone.calendar_time_zone(Some("Europe/London"));

        assert_eq!(zone_name(&calendar_time_zone), Some("Europe/London"));
    }

    #[test]
    fn uses_the_request_zone_when_the_calendar_follows_the_device() {
        let request_time_zone = RequestTimeZone(parse_time_zone("Asia/Manila"));

        assert_eq!(
            zone_name(&request_time_zone.calendar_time_zone(None)),
            Some("Asia/Manila")
        );
    }

    #[test]
    fn falls_back_to_utc_without_a_stored_or_request_zone() {
        assert_eq!(
            zone_name(&RequestTimeZone(None).calendar_time_zone(None)),
            Some("UTC")
        );
    }

    #[test]
    fn has_no_zone_without_the_header() {
        assert!(read_time_zone_header(&HeaderMap::new()).is_none());
    }
}
