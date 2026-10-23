use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub allowed_app_origins: Vec<AllowedOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowedOrigin {
    Exact(String),
    Pattern { prefix: String, suffix: String },
}

#[derive(Debug, thiserror::Error)]
pub enum AppConfigError {
    #[error("environment variable {0} must be set")]
    Missing(&'static str),
    #[error("{0} is not a valid app origin")]
    InvalidOrigin(String),
}

impl AppConfig {
    pub fn from_environment() -> Result<Self, AppConfigError> {
        Ok(Self {
            database_url: read_required_variable("DATABASE_URL")?,
            allowed_app_origins: parse_allowed_origins(&read_required_variable("APP_ORIGINS")?)?,
        })
    }

    pub fn is_allowed_app_origin(&self, request_origin: &str) -> bool {
        self.allowed_app_origins
            .iter()
            .any(|allowed_origin| allowed_origin.matches(request_origin))
    }
}

impl AllowedOrigin {
    pub fn parse(origin_text: &str) -> Result<Self, AppConfigError> {
        let trimmed_origin = origin_text.trim();
        let is_http_origin =
            trimmed_origin.starts_with("https://") || trimmed_origin.starts_with("http://");
        let has_path = trimmed_origin
            .split_once("://")
            .is_some_and(|(_, host_and_rest)| host_and_rest.contains('/'));
        if !is_http_origin || has_path || trimmed_origin.matches('*').count() > 1 {
            return Err(AppConfigError::InvalidOrigin(trimmed_origin.to_owned()));
        }
        Ok(match trimmed_origin.split_once('*') {
            Some((prefix, suffix)) => AllowedOrigin::Pattern {
                prefix: prefix.to_owned(),
                suffix: suffix.to_owned(),
            },
            None => AllowedOrigin::Exact(trimmed_origin.to_owned()),
        })
    }

    pub fn matches(&self, request_origin: &str) -> bool {
        match self {
            AllowedOrigin::Exact(exact_origin) => exact_origin == request_origin,
            AllowedOrigin::Pattern { prefix, suffix } => request_origin
                .strip_prefix(prefix.as_str())
                .and_then(|origin_rest| origin_rest.strip_suffix(suffix.as_str()))
                .is_some_and(is_wildcard_segment),
        }
    }
}

fn is_wildcard_segment(wildcard_segment: &str) -> bool {
    !wildcard_segment.is_empty()
        && wildcard_segment
            .bytes()
            .all(|segment_byte| segment_byte.is_ascii_alphanumeric() || segment_byte == b'-')
}

fn parse_allowed_origins(origins_text: &str) -> Result<Vec<AllowedOrigin>, AppConfigError> {
    origins_text
        .split(',')
        .filter(|origin_text| !origin_text.trim().is_empty())
        .map(AllowedOrigin::parse)
        .collect()
}

fn read_required_variable(variable_name: &'static str) -> Result<String, AppConfigError> {
    env::var(variable_name)
        .ok()
        .filter(|variable_value| !variable_value.trim().is_empty())
        .ok_or(AppConfigError::Missing(variable_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREVIEW_PATTERN: &str = "https://dai-bx-web-*-shans-projects-a71057cb.vercel.app";

    #[test]
    fn parses_a_comma_separated_list_of_origins() {
        let allowed_origins = parse_allowed_origins(&format!(
            "https://dai-bx-web.vercel.app, {PREVIEW_PATTERN},"
        ))
        .expect("both origins should parse");

        assert_eq!(allowed_origins.len(), 2);
        assert_eq!(
            allowed_origins[0],
            AllowedOrigin::Exact(String::from("https://dai-bx-web.vercel.app"))
        );
    }

    #[test]
    fn rejects_an_origin_with_a_path_or_two_wildcards_or_no_scheme() {
        assert!(AllowedOrigin::parse("https://dai-bx-web.vercel.app/backend").is_err());
        assert!(AllowedOrigin::parse("https://*-*.vercel.app").is_err());
        assert!(AllowedOrigin::parse("dai-bx-web.vercel.app").is_err());
    }

    #[test]
    fn matches_an_exact_origin_only_exactly() {
        let exact_origin = AllowedOrigin::parse("http://localhost:3000").expect("should parse");

        assert!(exact_origin.matches("http://localhost:3000"));
        assert!(!exact_origin.matches("http://localhost:3001"));
        assert!(!exact_origin.matches("https://localhost:3000"));
    }

    #[test]
    fn matches_a_preview_origin_through_the_wildcard() {
        let preview_origin = AllowedOrigin::parse(PREVIEW_PATTERN).expect("should parse");

        assert!(
            preview_origin.matches("https://dai-bx-web-abc123-shans-projects-a71057cb.vercel.app")
        );
        assert!(
            preview_origin
                .matches("https://dai-bx-web-git-release-v300-shans-projects-a71057cb.vercel.app")
        );
    }

    #[test]
    fn refuses_a_wildcard_match_that_is_empty_or_spans_a_dot_or_slash() {
        let preview_origin = AllowedOrigin::parse(PREVIEW_PATTERN).expect("should parse");

        assert!(!preview_origin.matches("https://dai-bx-web--shans-projects-a71057cb.vercel.app"));
        assert!(
            !preview_origin
                .matches("https://dai-bx-web-evil.example-shans-projects-a71057cb.vercel.app")
        );
        assert!(
            !preview_origin.matches("https://dai-bx-web-a/b-shans-projects-a71057cb.vercel.app")
        );
    }
}
