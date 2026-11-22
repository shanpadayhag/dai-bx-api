use crate::features::search::error::SearchError;

const QUERY_LENGTH_MAX: usize = 200;
const WORD_LENGTH_MAX: usize = 50;
const WORD_COUNT_MAX: usize = 6;
const LIMIT_DEFAULT: i64 = 5;
const LIMIT_MAX: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    All,
    Tasks,
    Groups,
    Cards,
    Events,
}

impl SearchKind {
    fn parse(kind_text: &str) -> Option<Self> {
        match kind_text {
            "all" => Some(Self::All),
            "tasks" => Some(Self::Tasks),
            "groups" => Some(Self::Groups),
            "cards" => Some(Self::Cards),
            "events" => Some(Self::Events),
            _ => None,
        }
    }

    pub fn includes(self, wanted_kind: SearchKind) -> bool {
        self == Self::All || self == wanted_kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub words: Vec<String>,
    pub kind: SearchKind,
    pub limit: i64,
}

impl SearchQuery {
    pub fn parse(
        query_text: Option<&str>,
        kind_text: Option<&str>,
        limit_text: Option<&str>,
    ) -> Result<Self, SearchError> {
        let trimmed_query_text = query_text.map(str::trim).unwrap_or_default();
        if trimmed_query_text.is_empty() {
            return Err(SearchError::Invalid("q is required"));
        }
        if trimmed_query_text.chars().count() > QUERY_LENGTH_MAX {
            return Err(SearchError::Invalid("q is at most 200 characters"));
        }
        let kind = kind_text
            .map(|kind_text| {
                SearchKind::parse(kind_text).ok_or(SearchError::Invalid(
                    "kind is all, tasks, groups, cards or events",
                ))
            })
            .transpose()?
            .unwrap_or(SearchKind::All);
        let limit = limit_text
            .map(|limit_text| {
                limit_text
                    .parse::<i64>()
                    .ok()
                    .filter(|limit| (1..=LIMIT_MAX).contains(limit))
                    .ok_or(SearchError::Invalid("limit is 1 to 50"))
            })
            .transpose()?
            .unwrap_or(LIMIT_DEFAULT);
        Ok(Self {
            words: split_search_words(trimmed_query_text),
            kind,
            limit,
        })
    }

    pub fn has_words(&self) -> bool {
        !self.words.is_empty()
    }
}

fn split_search_words(query_text: &str) -> Vec<String> {
    let mut search_words: Vec<String> = Vec::with_capacity(WORD_COUNT_MAX);
    for raw_word in query_text
        .split(|query_character: char| !query_character.is_alphanumeric())
        .filter(|raw_word| !raw_word.is_empty())
    {
        let search_word: String = raw_word
            .chars()
            .take(WORD_LENGTH_MAX)
            .collect::<String>()
            .to_lowercase();
        if !search_words.contains(&search_word) {
            search_words.push(search_word);
        }
        if search_words.len() == WORD_COUNT_MAX {
            break;
        }
    }
    search_words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(query_text: &str) -> Vec<String> {
        SearchQuery::parse(Some(query_text), None, None)
            .expect("query should parse")
            .words
    }

    #[test]
    fn splits_lowercases_and_trims_words() {
        assert_eq!(parse_words("  Rev PLAN  "), vec!["rev", "plan"]);
    }

    #[test]
    fn keeps_accented_and_chinese_words() {
        assert_eq!(parse_words("Café 中文学习"), vec!["café", "中文学习"]);
    }

    #[test]
    fn drops_emoji_and_punctuation_between_words() {
        assert_eq!(parse_words("gym🏋️,swim—run"), vec!["gym", "swim", "run"]);
    }

    #[test]
    fn never_keeps_search_operator_characters() {
        let operator_words = parse_words("a&b | !c :* 'd' (e)");
        assert_eq!(operator_words, vec!["a", "b", "c", "d", "e"]);
        assert!(
            operator_words
                .iter()
                .all(|operator_word| { operator_word.chars().all(char::is_alphanumeric) })
        );
    }

    #[test]
    fn cuts_long_words_at_50_characters() {
        let long_word = "a".repeat(80);
        assert_eq!(parse_words(&long_word), vec!["a".repeat(50)]);
    }

    #[test]
    fn drops_repeated_words() {
        assert_eq!(parse_words("plan Plan PLAN review"), vec!["plan", "review"]);
    }

    #[test]
    fn keeps_at_most_six_words() {
        assert_eq!(
            parse_words("one two three four five six seven eight"),
            vec!["one", "two", "three", "four", "five", "six"]
        );
    }

    #[test]
    fn punctuation_only_input_gives_no_words() {
        let punctuation_query =
            SearchQuery::parse(Some("— !! ..."), None, None).expect("query should parse");
        assert!(!punctuation_query.has_words());
    }

    #[test]
    fn rejects_a_missing_or_blank_query() {
        assert!(matches!(
            SearchQuery::parse(None, None, None),
            Err(SearchError::Invalid("q is required"))
        ));
        assert!(matches!(
            SearchQuery::parse(Some("   "), None, None),
            Err(SearchError::Invalid("q is required"))
        ));
    }

    #[test]
    fn accepts_200_characters_and_rejects_201() {
        let longest_query = "é".repeat(200);
        assert!(SearchQuery::parse(Some(&longest_query), None, None).is_ok());
        let too_long_query = "é".repeat(201);
        assert!(matches!(
            SearchQuery::parse(Some(&too_long_query), None, None),
            Err(SearchError::Invalid("q is at most 200 characters"))
        ));
    }

    #[test]
    fn defaults_to_every_kind_and_five_results() {
        let default_query =
            SearchQuery::parse(Some("plan"), None, None).expect("query should parse");
        assert_eq!(default_query.kind, SearchKind::All);
        assert_eq!(default_query.limit, 5);
    }

    #[test]
    fn reads_each_kind() {
        for (kind_text, expected_kind) in [
            ("all", SearchKind::All),
            ("tasks", SearchKind::Tasks),
            ("groups", SearchKind::Groups),
            ("cards", SearchKind::Cards),
            ("events", SearchKind::Events),
        ] {
            let kind_query =
                SearchQuery::parse(Some("plan"), Some(kind_text), None).expect("kind should parse");
            assert_eq!(kind_query.kind, expected_kind);
        }
    }

    #[test]
    fn rejects_an_unknown_kind() {
        assert!(matches!(
            SearchQuery::parse(Some("plan"), Some("boards"), None),
            Err(SearchError::Invalid(
                "kind is all, tasks, groups, cards or events"
            ))
        ));
    }

    #[test]
    fn accepts_limits_from_1_to_50() {
        for limit_text in ["1", "50"] {
            assert!(SearchQuery::parse(Some("plan"), None, Some(limit_text)).is_ok());
        }
        for limit_text in ["0", "51", "-1", "five"] {
            assert!(matches!(
                SearchQuery::parse(Some("plan"), None, Some(limit_text)),
                Err(SearchError::Invalid("limit is 1 to 50"))
            ));
        }
    }

    #[test]
    fn all_includes_every_kind_and_a_single_kind_only_itself() {
        assert!(SearchKind::All.includes(SearchKind::Cards));
        assert!(SearchKind::Cards.includes(SearchKind::Cards));
        assert!(!SearchKind::Cards.includes(SearchKind::Tasks));
    }
}
