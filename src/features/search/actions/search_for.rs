use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::search::data_sources::postgres::search_source::{
    MatchedSeries, PostgresSearchSource,
};
use crate::features::search::entities::search_query::{SearchKind, SearchQuery};
use crate::features::search::entities::search_results::{EventMatch, SearchResults, SearchSection};
use crate::features::search::error::SearchError;

pub struct SearchFor<'a> {
    db_pool: &'a PgPool,
    postgres_search_source: PostgresSearchSource,
}

impl<'a> SearchFor<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_search_source: PostgresSearchSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        search_query: &SearchQuery,
        now: UtcInstant,
    ) -> Result<SearchResults, SearchError> {
        let search_kind = search_query.kind;
        if !search_query.has_words() {
            return Ok(SearchResults {
                tasks: search_kind
                    .includes(SearchKind::Tasks)
                    .then(SearchSection::empty),
                groups: search_kind
                    .includes(SearchKind::Groups)
                    .then(SearchSection::empty),
                cards: search_kind
                    .includes(SearchKind::Cards)
                    .then(SearchSection::empty),
                events: search_kind
                    .includes(SearchKind::Events)
                    .then(SearchSection::empty),
            });
        }
        let search_words = search_query.words.as_slice();
        let fetch_limit = search_query.limit + 1;
        let tasks = if search_kind.includes(SearchKind::Tasks) {
            let task_matches = self
                .postgres_search_source
                .search_tasks(self.db_pool, user_id, search_words, fetch_limit)
                .await?;
            Some(SearchSection::from_fetched(
                task_matches,
                search_query.limit,
            ))
        } else {
            None
        };
        let groups = if search_kind.includes(SearchKind::Groups) {
            let group_matches = self
                .postgres_search_source
                .search_groups(self.db_pool, user_id, search_words, fetch_limit)
                .await?;
            Some(SearchSection::from_fetched(
                group_matches,
                search_query.limit,
            ))
        } else {
            None
        };
        let cards = if search_kind.includes(SearchKind::Cards) {
            let card_matches = self
                .postgres_search_source
                .search_cards(self.db_pool, user_id, search_words, fetch_limit)
                .await?;
            Some(SearchSection::from_fetched(
                card_matches,
                search_query.limit,
            ))
        } else {
            None
        };
        let events = if search_kind.includes(SearchKind::Events) {
            Some(
                self.search_events(user_id, search_words, now, search_query.limit)
                    .await?,
            )
        } else {
            None
        };
        Ok(SearchResults {
            tasks,
            groups,
            cards,
            events,
        })
    }

    async fn search_events(
        &self,
        user_id: Uuid,
        search_words: &[String],
        now: UtcInstant,
        limit: i64,
    ) -> Result<SearchSection<EventMatch>, SearchError> {
        let mut event_matches = self
            .postgres_search_source
            .search_single_events(self.db_pool, user_id, search_words, now, limit + 1)
            .await?;
        let matched_series = self
            .postgres_search_source
            .search_series(self.db_pool, user_id, search_words, now, limit + 1)
            .await?;
        event_matches.extend(
            matched_series
                .into_iter()
                .filter_map(|matched_series| series_event_match(matched_series, now)),
        );
        event_matches
            .sort_by(|first_match, second_match| first_match.compare_for_search(second_match, now));
        Ok(SearchSection::from_fetched(event_matches, limit))
    }
}

fn series_event_match(matched_series: MatchedSeries, now: UtcInstant) -> Option<EventMatch> {
    let MatchedSeries {
        series,
        category_color,
        match_rank,
    } = matched_series;
    let occurrence_start = series.nearest_occurrence_start(now)?;
    Some(EventMatch {
        id: series.id,
        series_id: Some(series.id),
        starts_at: occurrence_start,
        ends_at: occurrence_start.plus_minutes(i64::from(series.duration_minutes)),
        title: series.title,
        category_color,
        time_zone: Some(series.time_zone),
        linked_task_id: None,
        match_rank,
    })
}
