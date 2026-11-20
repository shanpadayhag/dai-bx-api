use jiff::civil::Time;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallClockWindow {
    pub start: Time,
    pub end: Time,
}

impl WallClockWindow {
    pub fn new(start: Time, end: Time) -> Option<Self> {
        (start != end).then_some(Self { start, end })
    }

    pub fn crosses_midnight(&self) -> bool {
        self.end <= self.start
    }
}

pub fn parse_wall_clock_time(time_text: &str) -> Option<Time> {
    let is_hours_and_minutes = time_text.len() == 5 && time_text.as_bytes().get(2) == Some(&b':');
    is_hours_and_minutes
        .then(|| time_text.parse::<Time>().ok())
        .flatten()
}

pub fn format_wall_clock_time(time: Time) -> String {
    time.strftime("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use jiff::civil::time;

    use super::*;

    #[test]
    fn reads_only_hours_and_minutes() {
        assert_eq!(parse_wall_clock_time("07:30"), Some(time(7, 30, 0, 0)));
        for rejected_text in ["7:30", "07:30:00", "24:00", "5pm", ""] {
            assert_eq!(
                parse_wall_clock_time(rejected_text),
                None,
                "{rejected_text}"
            );
        }
    }

    #[test]
    fn a_window_needs_two_different_times_and_may_cross_midnight() {
        assert_eq!(
            WallClockWindow::new(time(8, 0, 0, 0), time(8, 0, 0, 0)),
            None
        );
        let night_window =
            WallClockWindow::new(time(22, 0, 0, 0), time(2, 0, 0, 0)).expect("a window");
        let day_window =
            WallClockWindow::new(time(8, 0, 0, 0), time(12, 0, 0, 0)).expect("a window");

        assert!(night_window.crosses_midnight());
        assert!(!day_window.crosses_midnight());
        assert_eq!(format_wall_clock_time(night_window.start), "22:00");
    }
}
