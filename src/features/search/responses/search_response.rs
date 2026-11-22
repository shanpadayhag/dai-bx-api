use serde::Serialize;

use crate::features::search::entities::search_results::{
    CardMatch, EventMatch, GroupMatch, SearchResults, SearchSection, TaskMatch,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSectionResponse<MatchResponse> {
    pub items: Vec<MatchResponse>,
    pub has_more: bool,
}

impl<Match, MatchResponse> From<SearchSection<Match>> for SearchSectionResponse<MatchResponse>
where
    MatchResponse: From<Match>,
{
    fn from(search_section: SearchSection<Match>) -> Self {
        Self {
            items: search_section
                .items
                .into_iter()
                .map(MatchResponse::from)
                .collect(),
            has_more: search_section.has_more,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMatchResponse {
    pub id: String,
    pub name: String,
    pub group_id: String,
    pub group_name: String,
    pub parent_names: Vec<String>,
    pub done: bool,
    pub running: bool,
}

impl From<TaskMatch> for TaskMatchResponse {
    fn from(task_match: TaskMatch) -> Self {
        Self {
            id: task_match.id.to_string(),
            name: task_match.name,
            group_id: task_match.group_id.to_string(),
            group_name: task_match.group_name,
            parent_names: task_match.parent_names,
            done: task_match.is_done,
            running: task_match.is_running,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMatchResponse {
    pub id: String,
    pub name: String,
    pub open_task_count: i64,
}

impl From<GroupMatch> for GroupMatchResponse {
    fn from(group_match: GroupMatch) -> Self {
        Self {
            id: group_match.id.to_string(),
            name: group_match.name,
            open_task_count: group_match.open_task_count,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardMatchResponse {
    pub id: String,
    pub title: String,
    pub board_id: String,
    pub board_name: String,
    pub list_name: String,
    pub archived: bool,
    pub running: bool,
    pub snippet: Option<String>,
}

impl From<CardMatch> for CardMatchResponse {
    fn from(card_match: CardMatch) -> Self {
        Self {
            id: card_match.id.to_string(),
            title: card_match.title,
            board_id: card_match.board_id.to_string(),
            board_name: card_match.board_name,
            list_name: card_match.list_name,
            archived: card_match.is_archived,
            running: card_match.is_running,
            snippet: card_match.snippet,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventMatchResponse {
    pub id: String,
    pub series_id: Option<String>,
    pub title: String,
    pub category_color: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
    pub time_zone: Option<String>,
    pub repeats: bool,
    pub task_id: Option<String>,
}

impl From<EventMatch> for EventMatchResponse {
    fn from(event_match: EventMatch) -> Self {
        Self {
            id: event_match.id.to_string(),
            series_id: event_match.series_id.map(|series_id| series_id.to_string()),
            title: event_match.title,
            category_color: event_match.category_color,
            starts_at: event_match.starts_at.to_iso_text(),
            ends_at: event_match.ends_at.to_iso_text(),
            time_zone: event_match.time_zone,
            repeats: event_match.series_id.is_some(),
            task_id: event_match
                .linked_task_id
                .map(|task_id| task_id.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<SearchSectionResponse<TaskMatchResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<SearchSectionResponse<GroupMatchResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cards: Option<SearchSectionResponse<CardMatchResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<SearchSectionResponse<EventMatchResponse>>,
}

impl From<SearchResults> for SearchResponse {
    fn from(search_results: SearchResults) -> Self {
        Self {
            tasks: search_results.tasks.map(SearchSectionResponse::from),
            groups: search_results.groups.map(SearchSectionResponse::from),
            cards: search_results.cards.map(SearchSectionResponse::from),
            events: search_results.events.map(SearchSectionResponse::from),
        }
    }
}
