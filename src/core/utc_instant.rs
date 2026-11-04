use std::time::{SystemTime, UNIX_EPOCH};

const MILLIS_PER_SECOND: i64 = 1_000;
const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const MILLIS_PER_DAY: i64 = 24 * 60 * MILLIS_PER_MINUTE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcInstant {
    unix_millis: i64,
}

impl UtcInstant {
    pub fn from_unix_millis(unix_millis: i64) -> Self {
        Self { unix_millis }
    }

    pub fn now() -> Self {
        let elapsed_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the system clock is after 1970")
            .as_millis();
        Self::from_unix_millis(i64::try_from(elapsed_millis).unwrap_or(i64::MAX))
    }

    pub fn parse(instant_text: &str) -> Option<Self> {
        let instant_bytes = instant_text.as_bytes();
        if !instant_text.is_ascii()
            || instant_bytes.len() < 20
            || instant_bytes[4] != b'-'
            || instant_bytes[7] != b'-'
            || instant_bytes[10] != b'T'
            || instant_bytes[13] != b':'
            || instant_bytes[16] != b':'
            || instant_bytes.last() != Some(&b'Z')
        {
            return None;
        }
        let year = parse_digits(&instant_text[0..4])?;
        let month = parse_digits(&instant_text[5..7])?;
        let day = parse_digits(&instant_text[8..10])?;
        let hour = parse_digits(&instant_text[11..13])?;
        let minute = parse_digits(&instant_text[14..16])?;
        let second = parse_digits(&instant_text[17..19])?;
        let fraction_millis = parse_fraction_millis(&instant_text[19..instant_text.len() - 1])?;
        if year == 0
            || !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        let day_count = days_from_civil(year, month, day);
        Some(Self::from_unix_millis(
            day_count * MILLIS_PER_DAY
                + (hour * 3_600 + minute * 60 + second) * MILLIS_PER_SECOND
                + fraction_millis,
        ))
    }

    pub fn unix_millis(self) -> i64 {
        self.unix_millis
    }

    pub fn plus_minutes(self, minute_count: i64) -> Self {
        Self::from_unix_millis(self.unix_millis + minute_count * MILLIS_PER_MINUTE)
    }

    pub fn plus_days(self, day_count: i64) -> Self {
        Self::from_unix_millis(self.unix_millis + day_count * MILLIS_PER_DAY)
    }

    pub fn to_iso_text(self) -> String {
        let day_count = self.unix_millis.div_euclid(MILLIS_PER_DAY);
        let millis_of_day = self.unix_millis.rem_euclid(MILLIS_PER_DAY);
        let (year, month, day) = civil_from_days(day_count);
        let second_of_day = millis_of_day / MILLIS_PER_SECOND;
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
            second_of_day / 3_600,
            second_of_day % 3_600 / 60,
            second_of_day % 60,
            millis_of_day % MILLIS_PER_SECOND,
        )
    }
}

fn parse_digits(digit_text: &str) -> Option<i64> {
    if digit_text.is_empty()
        || !digit_text
            .bytes()
            .all(|digit_byte| digit_byte.is_ascii_digit())
    {
        return None;
    }
    digit_text.parse().ok()
}

fn parse_fraction_millis(fraction_text: &str) -> Option<i64> {
    if fraction_text.is_empty() {
        return Some(0);
    }
    let fraction_digits = fraction_text.strip_prefix('.')?;
    if fraction_digits.len() > 9 {
        return None;
    }
    parse_digits(fraction_digits)?;
    let padded_millis = format!("{fraction_digits:0<3}");
    parse_digits(&padded_millis[0..3])
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(day_count: i64) -> (i64, i64, i64) {
    let shifted_days = day_count + 719_468;
    let era = shifted_days.div_euclid(146_097);
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_whole_second_utc_time() {
        let parsed_instant = UtcInstant::parse("2026-11-05T09:30:15Z").expect("a valid time");

        assert_eq!(parsed_instant.unix_millis(), 1_793_871_015_000);
    }

    #[test]
    fn reads_fractions_down_to_the_millisecond() {
        let instant_millis = |instant_text| {
            UtcInstant::parse(instant_text)
                .expect("a valid time")
                .unix_millis()
                % 1_000
        };

        assert_eq!(instant_millis("2026-11-05T09:30:15.5Z"), 500);
        assert_eq!(instant_millis("2026-11-05T09:30:15.123Z"), 123);
        assert_eq!(instant_millis("2026-11-05T09:30:15.123999Z"), 123);
    }

    #[test]
    fn writes_the_same_text_javascript_writes() {
        for instant_text in [
            "2026-11-05T09:30:15.000Z",
            "1970-01-01T00:00:00.000Z",
            "2024-02-29T23:59:59.999Z",
            "1969-12-31T23:59:59.001Z",
            "2400-03-01T00:00:00.000Z",
        ] {
            let parsed_instant = UtcInstant::parse(instant_text).expect("a valid time");
            assert_eq!(parsed_instant.to_iso_text(), instant_text);
        }
    }

    #[test]
    fn refuses_text_that_is_not_a_utc_time() {
        for rejected_text in [
            "",
            "2026-11-05",
            "2026-11-05T09:30:15",
            "2026-11-05T09:30:15+08:00",
            "2026-11-05 09:30:15Z",
            "2026-11-05T09:30Z",
            "2026-13-05T09:30:15Z",
            "2026-02-29T09:30:15Z",
            "2026-11-31T09:30:15Z",
            "2026-11-05T24:00:00Z",
            "2026-11-05T09:60:15Z",
            "2026-11-05T09:30:60Z",
            "2026-11-05T09:30:15.Z",
            "2026-11-05T09:30:15.1234567890Z",
            "2026-11-05T09:30:15,5Z",
            "0000-01-01T00:00:00Z",
            "2026-1１-05T09:30:15Z",
            "+026-11-05T09:30:15Z",
        ] {
            assert_eq!(UtcInstant::parse(rejected_text), None, "{rejected_text}");
        }
    }

    #[test]
    fn adds_minutes_and_days() {
        let start = UtcInstant::parse("2026-11-05T23:50:00Z").expect("a valid time");

        assert_eq!(
            start.plus_minutes(15).to_iso_text(),
            "2026-11-06T00:05:00.000Z"
        );
        assert_eq!(
            start.plus_days(42).to_iso_text(),
            "2026-12-17T23:50:00.000Z"
        );
    }

    #[test]
    fn orders_instants_by_time() {
        let earlier = UtcInstant::parse("2026-11-05T09:00:00Z").expect("a valid time");
        let later = UtcInstant::parse("2026-11-05T09:00:00.001Z").expect("a valid time");

        assert!(earlier < later);
    }
}
