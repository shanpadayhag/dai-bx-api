pub const IDLE_LENGTH_HINT: &str = "Use days, weeks or months, like 3d, 2w or 1m";

const MONTHS_IN_A_YEAR: i32 = 12;
const DAYS_IN_A_YEAR: i32 = 365;
const DAYS_IN_A_WEEK: i32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleLength {
    pub months: i32,
    pub days: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleUnit {
    Month,
    Week,
    Day,
}

impl IdleLength {
    pub fn parse(length_text: &str) -> Option<Self> {
        let mut seen_units: Vec<IdleUnit> = Vec::with_capacity(3);
        let mut idle_length = Self { months: 0, days: 0 };
        for length_part in length_text.split_whitespace() {
            let (amount, idle_unit) = parse_part(&length_part.to_ascii_lowercase())?;
            if seen_units.contains(&idle_unit) {
                return None;
            }
            seen_units.push(idle_unit);
            match idle_unit {
                IdleUnit::Month => idle_length.months = amount,
                IdleUnit::Week => {
                    idle_length.days = idle_length
                        .days
                        .checked_add(amount.checked_mul(DAYS_IN_A_WEEK)?)?;
                }
                IdleUnit::Day => idle_length.days = idle_length.days.checked_add(amount)?,
            }
        }
        idle_length.is_within_a_year().then_some(idle_length)
    }

    pub fn to_text(self) -> String {
        let week_count = self.days / DAYS_IN_A_WEEK;
        let day_count = self.days % DAYS_IN_A_WEEK;
        [(self.months, "m"), (week_count, "w"), (day_count, "d")]
            .into_iter()
            .filter(|(amount, _)| *amount > 0)
            .map(|(amount, unit_text)| format!("{amount}{unit_text}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn is_within_a_year(self) -> bool {
        let is_at_least_a_day = self.months > 0 || self.days > 0;
        let is_at_most_a_year = if self.months == 0 {
            self.days <= DAYS_IN_A_YEAR
        } else {
            (self.months < MONTHS_IN_A_YEAR && self.months * 30 + self.days <= DAYS_IN_A_YEAR)
                || (self.months == MONTHS_IN_A_YEAR && self.days == 0)
        };
        is_at_least_a_day && is_at_most_a_year
    }
}

fn parse_part(length_part: &str) -> Option<(i32, IdleUnit)> {
    let digit_count = length_part.bytes().take_while(u8::is_ascii_digit).count();
    let (amount_text, unit_text) = length_part.split_at(digit_count);
    let idle_unit = match unit_text {
        "m" | "mo" => IdleUnit::Month,
        "w" => IdleUnit::Week,
        "d" => IdleUnit::Day,
        _ => return None,
    };
    let amount = amount_text
        .parse::<i32>()
        .ok()
        .filter(|amount| *amount >= 1)?;
    Some((amount, idle_unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tidied(length_text: &str) -> Option<String> {
        IdleLength::parse(length_text).map(IdleLength::to_text)
    }

    #[test]
    fn accepts_each_unit_on_its_own() {
        assert_eq!(tidied("3d").as_deref(), Some("3d"));
        assert_eq!(tidied("2w").as_deref(), Some("2w"));
        assert_eq!(tidied("1m").as_deref(), Some("1m"));
        assert_eq!(tidied("1mo").as_deref(), Some("1m"));
    }

    #[test]
    fn combines_units_in_any_order_and_any_case() {
        assert_eq!(tidied("1m 2w").as_deref(), Some("1m 2w"));
        assert_eq!(tidied("2W 1MO").as_deref(), Some("1m 2w"));
        assert_eq!(tidied("  3d   1w ").as_deref(), Some("1w 3d"));
    }

    #[test]
    fn tidies_days_into_weeks() {
        assert_eq!(tidied("10d").as_deref(), Some("1w 3d"));
        assert_eq!(tidied("14d").as_deref(), Some("2w"));
    }

    #[test]
    fn keeps_months_and_days_apart_for_calendar_months() {
        assert_eq!(
            IdleLength::parse("1m 10d"),
            Some(IdleLength {
                months: 1,
                days: 10
            })
        );
    }

    #[test]
    fn accepts_a_year_at_most() {
        assert_eq!(tidied("12m").as_deref(), Some("12m"));
        assert_eq!(tidied("365d").as_deref(), Some("52w 1d"));
        assert_eq!(tidied("11m 5w").as_deref(), Some("11m 5w"));
    }

    #[test]
    fn refuses_anything_else() {
        for refused_text in [
            "",
            "   ",
            "0d",
            "3x",
            "d",
            "3",
            "1d 2d",
            "1w 1w",
            "1m 1mo",
            "13m",
            "12m 1d",
            "366d",
            "11m 6w",
            "1.5w",
            "-1d",
            "1 d",
            "99999999999d",
        ] {
            assert_eq!(IdleLength::parse(refused_text), None, "{refused_text:?}");
        }
    }
}
