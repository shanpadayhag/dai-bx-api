use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::activity::entities::activity_entry::ActivityEntry;
use crate::features::activity::error::ActivityError;

const ACTIVITY_PAGE_LIMIT_DEFAULT: i64 = 50;
const ACTIVITY_PAGE_LIMIT_MAX: i64 = 100;
const CURSOR_SEPARATOR: char = '~';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityCursor {
    pub occurred_at: UtcInstant,
    pub entry_id: Uuid,
}

impl ActivityCursor {
    pub fn after(activity_entry: &ActivityEntry) -> Self {
        Self {
            occurred_at: activity_entry.occurred_at,
            entry_id: activity_entry.id,
        }
    }

    pub fn to_text(self) -> String {
        format!(
            "{}{CURSOR_SEPARATOR}{}",
            self.occurred_at.unix_millis(),
            self.entry_id
        )
    }

    pub fn parse(cursor_text: &str) -> Option<Self> {
        let (millis_text, entry_id_text) = cursor_text.split_once(CURSOR_SEPARATOR)?;
        let occurred_millis = millis_text
            .parse::<i64>()
            .ok()
            .filter(|millis| *millis >= 0)?;
        let entry_id = Uuid::parse_str(entry_id_text).ok()?;
        Some(Self {
            occurred_at: UtcInstant::from_unix_millis(occurred_millis),
            entry_id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityQuery {
    pub since: Option<UtcInstant>,
    pub cursor: Option<ActivityCursor>,
    pub limit: i64,
}

impl ActivityQuery {
    pub fn parse(
        since_text: Option<&str>,
        cursor_text: Option<&str>,
        limit_text: Option<&str>,
    ) -> Result<Self, ActivityError> {
        let since = since_text
            .map(|since_text| {
                UtcInstant::parse(since_text)
                    .ok_or_else(|| ActivityError::Invalid(String::from("since is a UTC instant")))
            })
            .transpose()?;
        let cursor = cursor_text
            .map(|cursor_text| {
                ActivityCursor::parse(cursor_text)
                    .ok_or_else(|| ActivityError::Invalid(String::from("cursor is not valid")))
            })
            .transpose()?;
        let limit = limit_text
            .map(|limit_text| {
                limit_text
                    .parse::<i64>()
                    .ok()
                    .filter(|limit| (1..=ACTIVITY_PAGE_LIMIT_MAX).contains(limit))
                    .ok_or_else(|| ActivityError::Invalid(String::from("limit is 1 to 100")))
            })
            .transpose()?
            .unwrap_or(ACTIVITY_PAGE_LIMIT_DEFAULT);
        Ok(Self {
            since,
            cursor,
            limit,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityPage {
    pub entries: Vec<ActivityEntry>,
    pub next_cursor: Option<ActivityCursor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_the_cursor_it_writes() {
        let cursor = ActivityCursor {
            occurred_at: UtcInstant::from_unix_millis(1_732_000_000_123),
            entry_id: Uuid::now_v7(),
        };

        assert_eq!(ActivityCursor::parse(&cursor.to_text()), Some(cursor));
    }

    #[test]
    fn refuses_a_malformed_cursor() {
        for cursor_text in [
            "",
            "123",
            "abc~00000000-0000-0000-0000-000000000000",
            "-5~00000000-0000-0000-0000-000000000000",
            "123~not-a-uuid",
        ] {
            assert_eq!(ActivityCursor::parse(cursor_text), None, "{cursor_text}");
        }
    }

    #[test]
    fn defaults_the_limit_to_fifty_and_bounds_it() {
        let default_query = ActivityQuery::parse(None, None, None).expect("defaults should parse");

        assert_eq!(default_query.limit, 50);
        assert!(ActivityQuery::parse(None, None, Some("100")).is_ok());
        for limit_text in ["0", "101", "ten"] {
            assert!(
                ActivityQuery::parse(None, None, Some(limit_text)).is_err(),
                "{limit_text}"
            );
        }
    }

    #[test]
    fn refuses_a_since_that_is_not_a_utc_instant() {
        assert!(ActivityQuery::parse(Some("2026-11-21T00:00:00Z"), None, None).is_ok());
        assert!(ActivityQuery::parse(Some("2026-11-21"), None, None).is_err());
    }
}
