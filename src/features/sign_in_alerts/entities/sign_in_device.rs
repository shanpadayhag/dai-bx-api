const UNKNOWN_DEVICE_LABEL: &str = "Unknown device";
const BROWSER_PATTERNS: [(&str, &str); 5] = [
    ("Edg/", "Edge"),
    ("OPR/", "Opera"),
    ("Firefox/", "Firefox"),
    ("Chrome/", "Chrome"),
    ("Safari/", "Safari"),
];
const SYSTEM_PATTERNS: [(&str, &str); 7] = [
    ("iPhone", "iOS"),
    ("iPad", "iOS"),
    ("Android", "Android"),
    ("Mac OS X", "macOS"),
    ("Macintosh", "macOS"),
    ("Windows", "Windows"),
    ("Linux", "Linux"),
];

fn find_label(user_agent: &str, label_patterns: &[(&str, &'static str)]) -> Option<&'static str> {
    label_patterns
        .iter()
        .find(|(label_pattern, _)| user_agent.contains(label_pattern))
        .map(|(_, label)| *label)
}

pub fn describe_user_agent(user_agent: Option<&str>) -> String {
    let Some(user_agent) = user_agent else {
        return String::from(UNKNOWN_DEVICE_LABEL);
    };
    match (
        find_label(user_agent, &BROWSER_PATTERNS),
        find_label(user_agent, &SYSTEM_PATTERNS),
    ) {
        (Some(browser_label), Some(system_label)) => format!("{browser_label} on {system_label}"),
        (Some(only_label), None) | (None, Some(only_label)) => only_label.to_owned(),
        (None, None) => String::from(UNKNOWN_DEVICE_LABEL),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_same_words_as_the_sessions_list() {
        for (user_agent, expected_label) in [
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36",
                "Chrome on macOS",
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                "Safari on iOS",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0",
                "Firefox on Windows",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36 Edg/131.0",
                "Edge on Windows",
            ),
            ("Mozilla/5.0 (X11; Linux x86_64)", "Linux"),
            ("Opera OPR/114.0", "Opera"),
            ("curl/8.4.0", "Unknown device"),
        ] {
            assert_eq!(describe_user_agent(Some(user_agent)), expected_label);
        }
    }

    #[test]
    fn calls_a_missing_user_agent_an_unknown_device() {
        assert_eq!(describe_user_agent(None), "Unknown device");
    }
}
