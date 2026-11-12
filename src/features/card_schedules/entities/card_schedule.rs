use jiff::civil::{DateTime, Time};
use uuid::Uuid;

use crate::core::recurrence::{
    RepeatRule, latest_at_or_before, next_after, occurrences, parse_local_moment, parse_time_zone,
    to_instant, to_local,
};
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::entities::board::NextMove;
use crate::features::card_schedules::error::ScheduleError;

pub const SCHEDULES_PER_CARD_MAX: i64 = 20;
pub const BLOCK_MINUTES_MIN: i32 = 15;
pub const BLOCK_MINUTES_MAX: i32 = 1_440;
const MILLIS_PER_MINUTE: i64 = 60_000;

fn invalid(invalid_message: &str) -> ScheduleError {
    ScheduleError::Invalid(String::from(invalid_message))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKind {
    Once,
    Repeating,
}

impl ScheduleKind {
    pub fn parse(kind_text: &str) -> Result<Self, ScheduleError> {
        match kind_text {
            "once" => Ok(Self::Once),
            "repeating" => Ok(Self::Repeating),
            _ => Err(invalid("a schedule's kind is once or repeating")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Repeating => "repeating",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleBlock {
    pub minutes: i32,
    pub category_id: Option<Uuid>,
    pub start_time: Option<Time>,
}

fn parse_block_start_time(start_time_text: &str) -> Result<Time, ScheduleError> {
    let is_hours_and_minutes =
        start_time_text.len() == 5 && start_time_text.as_bytes().get(2) == Some(&b':');
    is_hours_and_minutes
        .then(|| start_time_text.parse::<Time>().ok())
        .flatten()
        .ok_or_else(|| invalid("a block start is a time like 10:00"))
}

impl ScheduleBlock {
    pub fn new(
        minutes: i64,
        category_id: Option<Uuid>,
        start_time_text: Option<&str>,
    ) -> Result<Self, ScheduleError> {
        let start_time = start_time_text.map(parse_block_start_time).transpose()?;
        i32::try_from(minutes)
            .ok()
            .filter(|minutes| (BLOCK_MINUTES_MIN..=BLOCK_MINUTES_MAX).contains(minutes))
            .map(|minutes| Self {
                minutes,
                category_id,
                start_time,
            })
            .ok_or_else(|| invalid("a calendar block is 15 minutes to 24 hours"))
    }

    fn local_start_on_run_day(self, run_local_start: DateTime) -> DateTime {
        self.start_time.map_or(run_local_start, |start_time| {
            run_local_start.date().to_datetime(start_time)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleTiming {
    pub time_zone: String,
    pub local_start: DateTime,
    pub repeat_rule: Option<RepeatRule>,
}

impl ScheduleTiming {
    pub fn from_input(
        kind: ScheduleKind,
        start_text: &str,
        input_zone_name: &str,
        schedule_zone_name: Option<&str>,
        repeat_rule: Option<RepeatRule>,
    ) -> Result<Self, ScheduleError> {
        let unknown_zone = || invalid("a time zone is an IANA name like Asia/Manila");
        let input_zone = parse_time_zone(input_zone_name).ok_or_else(unknown_zone)?;
        let time_zone = schedule_zone_name.unwrap_or(input_zone_name).to_owned();
        let schedule_zone = parse_time_zone(&time_zone).ok_or_else(unknown_zone)?;
        let typed_start = parse_local_moment(start_text)
            .ok_or_else(|| invalid("a start is a local time like 2026-11-09T18:00"))?;
        let local_start = to_instant(typed_start, &input_zone)
            .and_then(|start_instant| to_local(start_instant, &schedule_zone))
            .ok_or_else(|| invalid("that start doesn't exist"))?;
        let repeat_rule = match (kind, repeat_rule) {
            (ScheduleKind::Once, None) => None,
            (ScheduleKind::Repeating, Some(repeat_rule)) => Some(repeat_rule),
            (ScheduleKind::Once, Some(_)) => {
                return Err(invalid("a one-time schedule doesn't repeat"));
            }
            (ScheduleKind::Repeating, None) => {
                return Err(invalid("a repeating schedule needs a repeat"));
            }
        };
        Ok(Self {
            time_zone,
            local_start,
            repeat_rule,
        })
    }

    pub fn kind(&self) -> ScheduleKind {
        if self.repeat_rule.is_some() {
            ScheduleKind::Repeating
        } else {
            ScheduleKind::Once
        }
    }

    pub fn start_instant(&self) -> Option<UtcInstant> {
        to_instant(self.local_start, &parse_time_zone(&self.time_zone)?)
    }

    pub fn next_run_after(&self, now: UtcInstant) -> Option<UtcInstant> {
        let start_instant = self.start_instant()?;
        match &self.repeat_rule {
            None => (start_instant > now).then_some(start_instant),
            Some(repeat_rule) => next_after(
                repeat_rule,
                self.local_start,
                &parse_time_zone(&self.time_zone)?,
                now.max(UtcInstant::from_unix_millis(
                    start_instant.unix_millis() - 1,
                )),
            ),
        }
    }

    pub fn latest_run_at_or_before(&self, now: UtcInstant) -> Option<UtcInstant> {
        let start_instant = self.start_instant()?;
        match &self.repeat_rule {
            None => (start_instant <= now).then_some(start_instant),
            Some(repeat_rule) => latest_at_or_before(
                repeat_rule,
                self.local_start,
                &parse_time_zone(&self.time_zone)?,
                now,
            ),
        }
    }

    pub fn runs_in(&self, from: UtcInstant, to: UtcInstant) -> Vec<UtcInstant> {
        let (Some(schedule_zone), Some(start_instant)) =
            (parse_time_zone(&self.time_zone), self.start_instant())
        else {
            return Vec::new();
        };
        match &self.repeat_rule {
            None => (from <= start_instant && start_instant < to)
                .then_some(start_instant)
                .into_iter()
                .collect(),
            Some(repeat_rule) => {
                occurrences(repeat_rule, self.local_start, &schedule_zone, from, to)
            }
        }
    }

    fn block_from(
        &self,
        card_title: &str,
        schedule_block: ScheduleBlock,
        first_run: UtcInstant,
    ) -> Option<NewScheduleBlock> {
        let schedule_zone = parse_time_zone(&self.time_zone)?;
        let block_local_start =
            schedule_block.local_start_on_run_day(to_local(first_run, &schedule_zone)?);
        match &self.repeat_rule {
            None => {
                let block_starts_at = to_instant(block_local_start, &schedule_zone)?;
                Some(NewScheduleBlock::Event {
                    id: Uuid::now_v7(),
                    title: card_title.to_owned(),
                    category_id: schedule_block.category_id,
                    time_zone: self.time_zone.clone(),
                    starts_at: block_starts_at,
                    ends_at: UtcInstant::from_unix_millis(
                        block_starts_at.unix_millis()
                            + i64::from(schedule_block.minutes) * MILLIS_PER_MINUTE,
                    ),
                })
            }
            Some(repeat_rule) => {
                let earlier_run_count = occurrences(
                    repeat_rule,
                    self.local_start,
                    &schedule_zone,
                    self.start_instant()?,
                    first_run,
                )
                .len();
                Some(NewScheduleBlock::Series {
                    id: Uuid::now_v7(),
                    title: card_title.to_owned(),
                    category_id: schedule_block.category_id,
                    time_zone: self.time_zone.clone(),
                    local_start: block_local_start,
                    duration_minutes: schedule_block.minutes,
                    repeat_rule: repeat_rule.after_used_occurrences(
                        i64::try_from(earlier_run_count).unwrap_or(i64::MAX),
                    ),
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleSettings {
    pub target_list_id: Uuid,
    pub timing: ScheduleTiming,
    pub block: Option<ScheduleBlock>,
}

impl ScheduleSettings {
    pub fn first_run_after(&self, now: UtcInstant) -> Result<UtcInstant, ScheduleError> {
        self.timing
            .next_run_after(now)
            .ok_or_else(|| match self.timing.kind() {
                ScheduleKind::Once => invalid("a one-time schedule needs a start in the future"),
                ScheduleKind::Repeating => invalid("this repeat has no run left"),
            })
    }

    pub fn block_start_time_text(&self) -> Option<String> {
        self.block
            .and_then(|schedule_block| schedule_block.start_time)
            .map(|start_time| start_time.strftime("%H:%M").to_string())
    }

    pub fn new_block(&self, card_title: &str, first_run: UtcInstant) -> Option<NewScheduleBlock> {
        self.timing.block_from(card_title, self.block?, first_run)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCardSchedule {
    pub id: Uuid,
    pub user_id: Uuid,
    pub card_id: Uuid,
    pub board_id: Uuid,
    pub settings: ScheduleSettings,
    pub next_run_at: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewScheduleBlock {
    Series {
        id: Uuid,
        title: String,
        category_id: Option<Uuid>,
        time_zone: String,
        local_start: DateTime,
        duration_minutes: i32,
        repeat_rule: RepeatRule,
    },
    Event {
        id: Uuid,
        title: String,
        category_id: Option<Uuid>,
        time_zone: String,
        starts_at: UtcInstant,
        ends_at: UtcInstant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleWrite {
    Written,
    UnknownLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSchedule {
    pub id: Uuid,
    pub card_id: Uuid,
    pub board_id: Uuid,
    pub card_title: String,
    pub target_list_id: Option<Uuid>,
    pub target_list_name: Option<String>,
    pub timing: ScheduleTiming,
    pub block: Option<ScheduleBlock>,
    pub is_paused: bool,
    pub next_run_at: Option<UtcInstant>,
}

impl CardSchedule {
    pub fn is_active(&self) -> bool {
        !self.is_paused && self.target_list_id.is_some() && self.next_run_at.is_some()
    }

    pub fn is_due(&self, now: UtcInstant) -> bool {
        self.is_active()
            && self
                .next_run_at
                .is_some_and(|next_run_at| next_run_at <= now)
    }

    pub fn moves_in(&self, from: UtcInstant, to: UtcInstant) -> Vec<ScheduledMove> {
        let (Some(next_run_at), Some(list_name), false) = (
            self.next_run_at,
            self.target_list_name.as_ref(),
            self.is_paused,
        ) else {
            return Vec::new();
        };
        self.timing
            .runs_in(from.max(next_run_at), to)
            .into_iter()
            .map(|move_at| ScheduledMove {
                schedule_id: self.id,
                card_id: self.card_id,
                card_title: self.card_title.clone(),
                board_id: self.board_id,
                list_name: list_name.clone(),
                at: move_at,
            })
            .collect()
    }

    pub fn settings(&self) -> Option<ScheduleSettings> {
        Some(ScheduleSettings {
            target_list_id: self.target_list_id?,
            timing: self.timing.clone(),
            block: self.block,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletedCards {
    Card(Uuid),
    BoardList(Uuid),
    Board(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueSchedule {
    pub schedule_id: Uuid,
    pub user_id: Uuid,
}

pub fn next_move_of(card_schedules: &[CardSchedule]) -> Option<NextMove> {
    card_schedules
        .iter()
        .filter(|card_schedule| card_schedule.is_active())
        .filter_map(|card_schedule| {
            Some(NextMove {
                at: card_schedule.next_run_at?,
                list_name: card_schedule.target_list_name.clone()?,
            })
        })
        .min_by_key(|next_move| next_move.at)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledMove {
    pub schedule_id: Uuid,
    pub card_id: Uuid,
    pub card_title: String,
    pub board_id: Uuid,
    pub list_name: String,
    pub at: UtcInstant,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid test time")
    }

    fn weekly_rule(ends_json: &str) -> RepeatRule {
        serde_json::from_str(&format!(
            r#"{{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{ends_json}}}"#
        ))
        .expect("a valid rule")
    }

    fn monday_timing(ends_json: &str) -> ScheduleTiming {
        ScheduleTiming::from_input(
            ScheduleKind::Repeating,
            "2026-11-09T18:00",
            "Asia/Manila",
            None,
            Some(weekly_rule(ends_json)),
        )
        .expect("a valid timing")
    }

    fn settings_with(timing: ScheduleTiming, block: Option<ScheduleBlock>) -> ScheduleSettings {
        ScheduleSettings {
            target_list_id: Uuid::now_v7(),
            timing,
            block,
        }
    }

    #[test]
    fn reads_a_typed_start_into_the_schedule_s_own_zone() {
        let timing = ScheduleTiming::from_input(
            ScheduleKind::Once,
            "2026-11-09T09:30",
            "Asia/Manila",
            Some("Europe/London"),
            None,
        )
        .expect("a valid timing");

        assert_eq!(timing.time_zone, "Europe/London");
        assert_eq!(
            timing.local_start,
            "2026-11-09T01:30:00"
                .parse::<DateTime>()
                .expect("a local time")
        );
        assert_eq!(
            timing.start_instant(),
            Some(instant("2026-11-09T01:30:00Z"))
        );
        assert_eq!(timing.kind(), ScheduleKind::Once);
    }

    #[test]
    fn refuses_a_kind_and_repeat_that_disagree_or_a_bad_zone_or_start() {
        for (kind, start_text, input_zone_name, repeat_rule) in [
            (
                ScheduleKind::Once,
                "2026-11-09T09:30",
                "UTC",
                Some(weekly_rule(r#"{"never":true}"#)),
            ),
            (ScheduleKind::Repeating, "2026-11-09T09:30", "UTC", None),
            (ScheduleKind::Once, "2026-11-09T09:30", "Mars/Olympus", None),
            (ScheduleKind::Once, "2026-11-09", "UTC", None),
        ] {
            assert!(
                ScheduleTiming::from_input(kind, start_text, input_zone_name, None, repeat_rule)
                    .is_err(),
                "{start_text} {input_zone_name}"
            );
        }
        assert!(ScheduleKind::parse("sometimes").is_err());
        assert!(ScheduleBlock::new(14, None, None).is_err());
        assert!(ScheduleBlock::new(1_441, None, None).is_err());
        assert!(ScheduleBlock::new(15, None, None).is_ok());
    }

    #[test]
    fn finds_the_next_run_from_the_start_or_from_now_whichever_is_later() {
        let timing = monday_timing(r#"{"never":true}"#);

        assert_eq!(
            timing.next_run_after(instant("2026-11-01T00:00:00Z")),
            Some(instant("2026-11-09T10:00:00Z"))
        );
        assert_eq!(
            timing.next_run_after(instant("2026-11-09T10:00:00Z")),
            Some(instant("2026-11-16T10:00:00Z"))
        );
        let once_timing = ScheduleTiming {
            repeat_rule: None,
            ..monday_timing(r#"{"never":true}"#)
        };
        assert_eq!(
            once_timing.next_run_after(instant("2026-11-09T09:59:00Z")),
            Some(instant("2026-11-09T10:00:00Z"))
        );
        assert_eq!(
            once_timing.next_run_after(instant("2026-11-09T10:00:00Z")),
            None
        );
        assert!(
            settings_with(once_timing, None)
                .first_run_after(instant("2026-11-10T00:00:00Z"))
                .is_err()
        );
    }

    #[test]
    fn starts_a_block_series_at_the_first_run_with_the_runs_left() {
        let schedule_settings = settings_with(
            monday_timing(r#"{"after":4}"#),
            Some(ScheduleBlock {
                minutes: 60,
                category_id: None,
                start_time: None,
            }),
        );

        let Some(NewScheduleBlock::Series {
            local_start,
            duration_minutes,
            repeat_rule,
            title,
            ..
        }) = schedule_settings.new_block("Weekly review", instant("2026-11-23T10:00:00Z"))
        else {
            panic!("a repeating schedule's block should be a series");
        };

        assert_eq!(title, "Weekly review");
        assert_eq!(
            local_start,
            "2026-11-23T18:00:00"
                .parse::<DateTime>()
                .expect("a local time")
        );
        assert_eq!(duration_minutes, 60);
        assert_eq!(
            serde_json::to_value(&repeat_rule).expect("the rule writes")["ends"],
            serde_json::json!({ "after": 2 })
        );
        assert_eq!(
            settings_with(monday_timing(r#"{"never":true}"#), None)
                .new_block("Weekly review", instant("2026-11-23T10:00:00Z")),
            None
        );
    }

    #[test]
    fn starts_a_block_at_its_own_time_on_the_run_s_day() {
        let schedule_settings = settings_with(
            monday_timing(r#"{"never":true}"#),
            Some(ScheduleBlock::new(60, None, Some("20:30")).expect("a valid block")),
        );

        let Some(NewScheduleBlock::Series { local_start, .. }) =
            schedule_settings.new_block("Weekly review", instant("2026-11-23T10:00:00Z"))
        else {
            panic!("a repeating schedule's block should be a series");
        };

        assert_eq!(
            local_start,
            "2026-11-23T20:30:00"
                .parse::<DateTime>()
                .expect("a local time")
        );
        assert_eq!(
            schedule_settings.block_start_time_text(),
            Some(String::from("20:30"))
        );
        assert!(ScheduleBlock::new(60, None, Some("8:30")).is_err());
    }

    #[test]
    fn lists_only_an_active_schedule_s_moves_from_its_next_run() {
        let schedule = CardSchedule {
            id: Uuid::now_v7(),
            card_id: Uuid::now_v7(),
            board_id: Uuid::now_v7(),
            card_title: String::from("Weekly review"),
            target_list_id: Some(Uuid::now_v7()),
            target_list_name: Some(String::from("To Do")),
            timing: monday_timing(r#"{"never":true}"#),
            block: None,
            is_paused: false,
            next_run_at: Some(instant("2026-11-16T10:00:00Z")),
        };
        let move_starts = |listed_schedule: &CardSchedule| -> Vec<UtcInstant> {
            listed_schedule
                .moves_in(
                    instant("2026-11-02T00:00:00Z"),
                    instant("2026-11-24T00:00:00Z"),
                )
                .iter()
                .map(|scheduled_move| scheduled_move.at)
                .collect()
        };

        assert_eq!(
            move_starts(&schedule),
            vec![
                instant("2026-11-16T10:00:00Z"),
                instant("2026-11-23T10:00:00Z")
            ]
        );
        assert!(schedule.is_active());
        for inactive_schedule in [
            CardSchedule {
                is_paused: true,
                ..schedule.clone()
            },
            CardSchedule {
                target_list_id: None,
                target_list_name: None,
                ..schedule.clone()
            },
            CardSchedule {
                next_run_at: None,
                ..schedule.clone()
            },
        ] {
            assert!(!inactive_schedule.is_active());
            assert_eq!(move_starts(&inactive_schedule), Vec::new());
        }
    }

    #[test]
    fn finds_the_latest_run_at_or_before_now() {
        let weekly_timing = monday_timing(r#"{"never":true}"#);
        let once_timing = ScheduleTiming {
            repeat_rule: None,
            ..weekly_timing.clone()
        };

        assert_eq!(
            weekly_timing.latest_run_at_or_before(instant("2026-11-30T00:00:00Z")),
            Some(instant("2026-11-23T10:00:00Z"))
        );
        assert_eq!(
            weekly_timing.latest_run_at_or_before(instant("2026-11-09T10:00:00Z")),
            Some(instant("2026-11-09T10:00:00Z"))
        );
        assert_eq!(
            weekly_timing.latest_run_at_or_before(instant("2026-11-09T09:59:59Z")),
            None
        );
        assert_eq!(
            once_timing.latest_run_at_or_before(instant("2027-01-01T00:00:00Z")),
            Some(instant("2026-11-09T10:00:00Z"))
        );
        assert_eq!(
            once_timing.latest_run_at_or_before(instant("2026-11-09T09:00:00Z")),
            None
        );
    }
}
