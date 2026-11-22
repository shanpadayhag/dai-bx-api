use std::cmp::Ordering;

use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSection<Match> {
    pub items: Vec<Match>,
    pub has_more: bool,
}

impl<Match> SearchSection<Match> {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            has_more: false,
        }
    }

    pub fn from_fetched(mut fetched_matches: Vec<Match>, limit: i64) -> Self {
        let shown_count = usize::try_from(limit).unwrap_or(0);
        let has_more = fetched_matches.len() > shown_count;
        fetched_matches.truncate(shown_count);
        Self {
            items: fetched_matches,
            has_more,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMatch {
    pub id: Uuid,
    pub name: String,
    pub group_id: Uuid,
    pub group_name: String,
    pub parent_names: Vec<String>,
    pub is_done: bool,
    pub is_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMatch {
    pub id: Uuid,
    pub name: String,
    pub open_task_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardMatch {
    pub id: Uuid,
    pub title: String,
    pub board_id: Uuid,
    pub board_name: String,
    pub list_name: String,
    pub is_archived: bool,
    pub is_running: bool,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventMatch {
    pub id: Uuid,
    pub series_id: Option<Uuid>,
    pub title: String,
    pub category_color: Option<String>,
    pub starts_at: UtcInstant,
    pub ends_at: UtcInstant,
    pub time_zone: Option<String>,
    pub linked_task_id: Option<Uuid>,
    pub match_rank: f32,
}

impl EventMatch {
    pub fn is_upcoming(&self, now: UtcInstant) -> bool {
        self.ends_at >= now
    }

    pub fn compare_for_search(&self, other_match: &EventMatch, now: UtcInstant) -> Ordering {
        let is_self_upcoming = self.is_upcoming(now);
        let is_other_upcoming = other_match.is_upcoming(now);
        is_other_upcoming
            .cmp(&is_self_upcoming)
            .then_with(|| {
                other_match
                    .match_rank
                    .partial_cmp(&self.match_rank)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                if is_self_upcoming {
                    self.starts_at.cmp(&other_match.starts_at)
                } else {
                    other_match.starts_at.cmp(&self.starts_at)
                }
            })
            .then_with(|| self.id.cmp(&other_match.id))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResults {
    pub tasks: Option<SearchSection<TaskMatch>>,
    pub groups: Option<SearchSection<GroupMatch>>,
    pub cards: Option<SearchSection<CardMatch>>,
    pub events: Option<SearchSection<EventMatch>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid test time")
    }

    fn event_match(starts_at_text: &str, match_rank: f32) -> EventMatch {
        let starts_at = instant(starts_at_text);
        EventMatch {
            id: Uuid::now_v7(),
            series_id: None,
            title: String::from("Planning review"),
            category_color: None,
            starts_at,
            ends_at: starts_at.plus_minutes(60),
            time_zone: None,
            linked_task_id: None,
            match_rank,
        }
    }

    #[test]
    fn a_section_keeps_the_limit_and_says_when_more_matched() {
        let fuller_section = SearchSection::from_fetched(vec![1, 2, 3], 2);
        assert_eq!(fuller_section.items, vec![1, 2]);
        assert!(fuller_section.has_more);
        let exact_section = SearchSection::from_fetched(vec![1, 2], 2);
        assert!(!exact_section.has_more);
    }

    #[test]
    fn upcoming_events_come_before_past_ones_nearest_first() {
        let now = instant("2026-11-23T10:00:00Z");
        let mut event_matches = [
            event_match("2026-11-20T09:00:00Z", 0.1),
            event_match("2026-11-30T09:00:00Z", 0.1),
            event_match("2026-11-24T09:00:00Z", 0.1),
            event_match("2026-11-22T09:00:00Z", 0.1),
            event_match("2026-11-23T09:30:00Z", 0.1),
        ];
        event_matches
            .sort_by(|first_match, second_match| first_match.compare_for_search(second_match, now));
        let sorted_starts: Vec<String> = event_matches
            .iter()
            .map(|event_match| event_match.starts_at.to_iso_text())
            .collect();
        assert_eq!(
            sorted_starts,
            vec![
                "2026-11-23T09:30:00.000Z",
                "2026-11-24T09:00:00.000Z",
                "2026-11-30T09:00:00.000Z",
                "2026-11-22T09:00:00.000Z",
                "2026-11-20T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn a_better_match_ranks_first_within_upcoming_events() {
        let now = instant("2026-11-23T10:00:00Z");
        let weaker_match = event_match("2026-11-24T09:00:00Z", 0.1);
        let stronger_match = event_match("2026-11-30T09:00:00Z", 0.5);
        assert_eq!(
            stronger_match.compare_for_search(&weaker_match, now),
            Ordering::Less
        );
    }
}
