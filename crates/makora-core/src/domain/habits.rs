//! When the user works, read from the session intervals in local time: per-day profiles, an
//! hour-by-weekday table, a summary of habits and plain-language observations.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Timelike, Utc};

use crate::shared::ids::CategoryId;

use super::types::{
    format_clock as clock, format_hour_ranges, DailyStat, MIN_DAYS_FOR_HABITS, DayProfile, Habits, HourRange, LongestFocus, Observation, TimelineBlock,
    TimelineDay,
};

/// A working day runs from 04:00 to 04:00, so work past midnight belongs to the evening before.
const DAY_START: TimeDelta = TimeDelta::hours(4);
const FOCUS_GAP_SECONDS: f64 = 300.;
const TIMELINE_GAP_SECONDS: f64 = 120.;
const ACTIVE_HOUR_SECONDS: f64 = 900.;
const SHORT_FOCUS_SECONDS: f64 = 1500.;
const START_SPREAD_MINUTES: u32 = 90;
const LATE_SHARE: f64 = 0.15;
const WEEKEND_SHARE: f64 = 0.2;
const LATE_HOURS: [usize; 6] = [22, 23, 0, 1, 2, 3];
const MAX_OBSERVATIONS: usize = 4;
const PEAK_HOURS: usize = 3;

#[derive(Debug, Clone)]
pub struct Interval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub category_id: Option<CategoryId>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
}

/// The part of an interval inside one local day or hour.
struct Piece {
    local: NaiveDateTime,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl Piece {
    fn seconds(&self) -> f64 {
        (self.end - self.start).as_seconds_f64()
    }

    fn day(&self) -> NaiveDate {
        self.local.date()
    }

    fn second_of_day(&self) -> f64 {
        f64::from(self.local.num_seconds_from_midnight())
    }
}

/// Cuts `start..end` wherever the local clock reaches `next(local)`.
fn pieces<Tz: TimeZone>(
    mut start: DateTime<Utc>,
    end: DateTime<Utc>,
    tz: &Tz,
    next: fn(NaiveDateTime) -> Option<NaiveDateTime>,
) -> impl Iterator<Item = Piece> + '_ {
    std::iter::from_fn(move || {
        if start >= end {
            return None;
        }
        let local = start.with_timezone(tz).naive_local();
        let cut = next(local)
            .and_then(|next| first_instant_at_or_after(next, tz))
            .filter(|&cut| cut > start)
            .map_or(end, |cut| cut.min(end));
        let piece = Piece { local, start, end: cut };
        start = cut;
        Some(piece)
    })
}

fn next_day(local: NaiveDateTime) -> Option<NaiveDateTime> {
    local.date().succ_opt().map(|day| day.and_time(NaiveTime::MIN))
}

fn next_hour(local: NaiveDateTime) -> Option<NaiveDateTime> {
    local.date().and_hms_opt(local.hour(), 0, 0).map(|hour| hour + TimeDelta::hours(1))
}

/// `local` in `tz`, or the first moment after it when a DST change skips it.
fn first_instant_at_or_after<Tz: TimeZone>(local: NaiveDateTime, tz: &Tz) -> Option<DateTime<Utc>> {
    (0..96)
        .map(|step| local + TimeDelta::minutes(15 * step))
        .find_map(|local| tz.from_local_datetime(&local).earliest())
        .map(|at| at.with_timezone(&Utc))
}

/// Time per calendar day in `tz`, oldest first; days without any time are left out.
pub fn daily_totals<Tz: TimeZone>(intervals: &[Interval], tz: &Tz) -> Vec<DailyStat> {
    let mut days: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for interval in intervals {
        for piece in pieces(interval.start, interval.end, tz, next_day) {
            *days.entry(piece.day()).or_default() += piece.seconds();
        }
    }
    days.into_iter()
        .map(|(day, total_seconds)| DailyStat {
            day: day.format("%Y-%m-%d").to_string(),
            total_seconds,
        })
        .collect()
}

/// Seconds per calendar weekday (Monday first) and local hour.
pub fn hour_table<Tz: TimeZone>(intervals: &[Interval], tz: &Tz) -> [[f64; 24]; 7] {
    let mut table = [[0.; 24]; 7];
    for interval in intervals {
        for piece in pieces(interval.start, interval.end, tz, next_hour) {
            let weekday = piece.day().weekday().num_days_from_monday() as usize;
            table[weekday][piece.local.hour() as usize] += piece.seconds();
        }
    }
    table
}

/// One profile per working day with any time, oldest first. `intervals` must be ordered by start.
pub fn day_profiles<Tz: TimeZone>(intervals: &[Interval], tz: &Tz) -> Vec<DayProfile> {
    struct Day {
        profile: DayProfile,
        streak_start: DateTime<Utc>,
        last_end: DateTime<Utc>,
        last_interval: usize,
    }

    let mut days: BTreeMap<NaiveDate, Day> = BTreeMap::new();
    for (ix, interval) in intervals.iter().enumerate() {
        for piece in pieces(interval.start, interval.end, tz, next_hour) {
            let seconds = piece.seconds();
            let date = (piece.local - DAY_START).date();
            let offset = (piece.local - date.and_time(NaiveTime::MIN)).as_seconds_f64();
            let from = (offset / 60.).floor() as u32;
            let to = ((offset + seconds) / 60.).ceil() as u32;
            let day = days.entry(date).or_insert_with(|| Day {
                profile: DayProfile {
                    day: date.format("%Y-%m-%d").to_string(),
                    weekday: date.weekday().num_days_from_monday() as u8,
                    first_active: from,
                    last_active: to,
                    active_seconds: 0.,
                    longest_focus_seconds: 0.,
                    sessions: 0,
                    hours: [0.; 24],
                },
                streak_start: piece.start,
                last_end: piece.start,
                last_interval: usize::MAX,
            });
            if (piece.start - day.last_end).as_seconds_f64() >= FOCUS_GAP_SECONDS {
                day.streak_start = piece.start;
            }
            day.last_end = day.last_end.max(piece.end);
            let profile = &mut day.profile;
            profile.hours[piece.local.hour() as usize] += seconds;
            profile.active_seconds += seconds;
            profile.first_active = profile.first_active.min(from);
            profile.last_active = profile.last_active.max(to);
            profile.longest_focus_seconds = profile
                .longest_focus_seconds
                .max((day.last_end - day.streak_start).as_seconds_f64());
            if day.last_interval != ix {
                profile.sessions += 1;
                day.last_interval = ix;
            }
        }
    }
    days.into_values().map(|day| day.profile).collect()
}

pub fn summarize(days: &[DayProfile], table: &[[f64; 24]; 7]) -> Habits {
    let total: f64 = days.iter().map(|d| d.active_seconds).sum();
    if days.is_empty() || total <= 0. {
        return Habits::default();
    }
    let by_hour: Vec<f64> = (0..24).map(|h| table.iter().map(|row| row[h]).sum()).collect();

    let active = (0..24).filter(|&h| median(days.iter().map(|d| d.hours[h])) >= ACTIVE_HOUR_SECONDS);

    let mut peak: Vec<usize> = (0..24).filter(|&h| by_hour[h] > 0.).collect();
    peak.sort_by(|&a, &b| by_hour[b].total_cmp(&by_hour[a]).then(a.cmp(&b)));
    peak.truncate(PEAK_HOURS);
    peak.sort_unstable();

    let mut starts: Vec<f64> = days.iter().map(|d| f64::from(d.first_active)).collect();
    starts.sort_by(f64::total_cmp);

    let sessions: u32 = days.iter().map(|d| d.sessions).sum();
    Habits {
        active_days: days.len(),
        active_hours: hour_ranges(active),
        typical_start: Some(median(starts.iter().copied()).round() as u32),
        typical_end: Some(median(days.iter().map(|d| f64::from(d.last_active))).round() as u32),
        start_quartiles: Some((
            quantile(&starts, 0.25).round() as u32,
            quantile(&starts, 0.75).round() as u32,
        )),
        peak_hours: hour_ranges(peak),
        late_share: LATE_HOURS.iter().map(|&h| by_hour[h]).sum::<f64>() / total,
        weekend_share: days
            .iter()
            .filter(|d| d.weekday >= 5)
            .map(|d| d.active_seconds)
            .sum::<f64>()
            / total,
        longest_focus: days
            .iter()
            .max_by(|a, b| a.longest_focus_seconds.total_cmp(&b.longest_focus_seconds))
            .map(|d| LongestFocus {
                day: d.day.clone(),
                seconds: d.longest_focus_seconds,
            }),
        median_focus_seconds: median(days.iter().map(|d| d.longest_focus_seconds)),
        fragmentation: f64::from(sessions) / (total / 3600.),
    }
}

/// At most four observations, the most telling first. None until there are three active days.
pub fn observations(habits: &Habits) -> Vec<Observation> {
    if habits.active_days < MIN_DAYS_FOR_HABITS {
        return Vec::new();
    }
    let mut found: Vec<(f64, Observation)> = Vec::new();
    if let Some((early, late)) = habits.start_quartiles {
        let spread = late.saturating_sub(early);
        if spread > START_SPREAD_MINUTES {
            found.push((
                f64::from(spread) / f64::from(START_SPREAD_MINUTES),
                observation(
                    format!("Your start time swings from {} to {}.", clock(early), clock(late)),
                    "A fixed first block at the same time each day makes starting easier.",
                ),
            ));
        }
    }
    if habits.late_share >= LATE_SHARE {
        found.push((
            habits.late_share / LATE_SHARE,
            observation(
                format!("{} of your time is late at night, after 22:00.", percent(habits.late_share)),
                "Move that work earlier or set a hard stop for the evening.",
            ),
        ));
    }
    if habits.longest_focus.is_some() && habits.median_focus_seconds < SHORT_FOCUS_SECONDS {
        found.push((
            SHORT_FOCUS_SECONDS / habits.median_focus_seconds.max(60.),
            observation(
                format!(
                    "Focus streaks are short; the longest one on a typical day is {}.",
                    minutes(habits.median_focus_seconds)
                ),
                "Batch messages and app switches into set times to protect longer blocks.",
            ),
        ));
    }
    if !habits.peak_hours.is_empty() {
        found.push((
            1.,
            observation(
                format!("You are most active {}.", format_hour_ranges(&habits.peak_hours)),
                "Keep deep work in those hours and move meetings and chores outside them.",
            ),
        ));
    }
    if habits.weekend_share >= WEEKEND_SHARE {
        found.push((
            habits.weekend_share / WEEKEND_SHARE,
            observation(
                format!("{} of your time falls on weekends.", percent(habits.weekend_share)),
                "Plan a full day off now and then to recharge.",
            ),
        ));
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().take(MAX_OBSERVATIONS).map(|(_, o)| o).collect()
}

/// One row per local day: blocks of time in one category, neighbours less than two minutes apart
/// merged. `intervals` must be ordered by start.
pub fn timeline<Tz: TimeZone>(intervals: &[Interval], tz: &Tz) -> Vec<TimelineDay> {
    let mut days: BTreeMap<NaiveDate, Vec<TimelineBlock>> = BTreeMap::new();
    for interval in intervals {
        for piece in pieces(interval.start, interval.end, tz, next_day) {
            let start = piece.second_of_day();
            let end = (start + piece.seconds()).min(86400.);
            let blocks = days.entry(piece.day()).or_default();
            match blocks.last_mut() {
                Some(last) if last.category_id == interval.category_id && start - last.end < TIMELINE_GAP_SECONDS => {
                    last.end = last.end.max(end);
                }
                _ => blocks.push(TimelineBlock {
                    start,
                    end,
                    category_id: interval.category_id,
                    category_name: interval.category_name.clone(),
                    category_color: interval.category_color.clone(),
                }),
            }
        }
    }
    days.into_iter()
        .map(|(day, blocks)| TimelineDay {
            day: day.format("%Y-%m-%d").to_string(),
            blocks,
        })
        .collect()
}

fn observation(text: String, suggestion: &str) -> Observation {
    Observation {
        text,
        suggestion: Some(suggestion.to_owned()),
    }
}

fn hour_ranges(hours: impl IntoIterator<Item = usize>) -> Vec<HourRange> {
    let mut ranges: Vec<HourRange> = Vec::new();
    for hour in hours {
        let hour = hour as u8;
        match ranges.last_mut() {
            Some(range) if range.end == hour => range.end = hour + 1,
            _ => ranges.push(HourRange { start: hour, end: hour + 1 }),
        }
    }
    ranges
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values: Vec<f64> = values.collect();
    values.sort_by(f64::total_cmp);
    quantile(&values, 0.5)
}

/// Linear interpolation between the closest ranks of sorted `values`.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.;
    };
    let rank = q * last as f64;
    let (low, high) = (rank.floor() as usize, rank.ceil() as usize);
    sorted[low] + (sorted[high] - sorted[low]) * (rank - low as f64)
}

fn percent(share: f64) -> String {
    format!("{:.0}%", share * 100.)
}

fn minutes(seconds: f64) -> String {
    let minutes = (seconds / 60.).round() as u32;
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {} min", minutes / 60, minutes % 60)
    }
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;
    use crate::domain::test_tz::{utc, MidnightDst};

    fn at(start: &str, end: &str) -> Interval {
        tagged(start, end, None)
    }

    fn tagged(start: &str, end: &str, category: Option<i64>) -> Interval {
        Interval {
            start: utc(start),
            end: utc(end),
            category_id: category.map(CategoryId),
            category_name: None,
            category_color: None,
        }
    }

    fn utc_zone() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn totals(days: &[DayProfile]) -> Vec<(&str, f64)> {
        days.iter().map(|d| (d.day.as_str(), d.active_seconds)).collect()
    }

    fn daily(stats: &[DailyStat]) -> Vec<(&str, f64)> {
        stats.iter().map(|d| (d.day.as_str(), d.total_seconds)).collect()
    }

    #[test]
    fn a_session_across_local_midnight_counts_towards_both_calendar_days() {
        let tz = FixedOffset::east_opt(7 * 3600).unwrap();
        // 23:00 to 01:30 local time.
        let stats = daily_totals(&[at("2026-03-01T16:00:00Z", "2026-03-01T18:30:00Z")], &tz);
        assert_eq!(daily(&stats), [("2026-03-01", 3600.), ("2026-03-02", 5400.)]);
    }

    #[test]
    fn work_past_midnight_belongs_to_the_working_day_before() {
        let tz = FixedOffset::east_opt(7 * 3600).unwrap();
        // 23:00 to 01:30, then 04:30 to 05:00 local time.
        let days = day_profiles(
            &[
                at("2026-03-01T16:00:00Z", "2026-03-01T18:30:00Z"),
                at("2026-03-01T21:30:00Z", "2026-03-01T22:00:00Z"),
            ],
            &tz,
        );
        assert_eq!(totals(&days), [("2026-03-01", 9000.), ("2026-03-02", 1800.)]);
        assert_eq!((days[0].first_active, days[0].last_active), (23 * 60, 25 * 60 + 30));
        assert_eq!((days[1].first_active, days[1].last_active), (4 * 60 + 30, 5 * 60));
        assert_eq!(days[0].longest_focus_seconds, 9000.);
        assert_eq!(clock(days[0].last_active), "1:30");
    }

    #[test]
    fn a_session_spanning_several_days_fills_each_one() {
        let stats = daily_totals(&[at("2026-03-01T12:00:00Z", "2026-03-03T06:00:00Z")], &utc_zone());
        assert_eq!(
            daily(&stats),
            [("2026-03-01", 43200.), ("2026-03-02", 86400.), ("2026-03-03", 21600.)]
        );
    }

    #[test]
    fn time_splits_at_local_hour_boundaries() {
        // UTC+5:30, so 04:00Z to 05:45Z is 09:30 to 11:15 local.
        let tz = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
        let days = day_profiles(&[at("2026-03-02T04:00:00Z", "2026-03-02T05:45:00Z")], &tz);
        assert_eq!(days[0].hours[9], 1800.);
        assert_eq!(days[0].hours[10], 3600.);
        assert_eq!(days[0].hours[11], 900.);
        assert_eq!(days[0].hours.iter().sum::<f64>(), 6300.);
    }

    #[test]
    fn a_dst_change_that_skips_midnight_still_splits_days_and_hours() {
        // 2026-03-28 (a Saturday) 23:30 local (+1) to 2026-03-29 02:30 local (+2).
        let intervals = [at("2026-03-28T22:30:00Z", "2026-03-29T00:30:00Z")];
        let stats = daily_totals(&intervals, &MidnightDst);
        assert_eq!(daily(&stats), [("2026-03-28", 1800.), ("2026-03-29", 5400.)]);
        let table = hour_table(&intervals, &MidnightDst);
        assert_eq!(table[5][23], 1800.);
        assert_eq!((table[6][0], table[6][1], table[6][2]), (0., 3600., 1800.));
    }

    #[test]
    fn an_empty_or_backwards_interval_counts_nothing() {
        let days = day_profiles(
            &[
                at("2026-03-01T09:00:00Z", "2026-03-01T09:00:00Z"),
                at("2026-03-01T09:00:00Z", "2026-03-01T08:00:00Z"),
            ],
            &utc_zone(),
        );
        assert!(days.is_empty());
    }

    #[test]
    fn focus_streaks_bridge_gaps_under_five_minutes() {
        let bridged = day_profiles(
            &[
                at("2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z"),
                at("2026-03-02T09:34:59Z", "2026-03-02T10:00:00Z"),
            ],
            &utc_zone(),
        );
        assert_eq!(bridged[0].longest_focus_seconds, 3600.);
        assert_eq!(bridged[0].sessions, 2);

        let broken = day_profiles(
            &[
                at("2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z"),
                at("2026-03-02T09:35:01Z", "2026-03-02T10:00:00Z"),
            ],
            &utc_zone(),
        );
        assert_eq!(broken[0].longest_focus_seconds, 1800.);
    }

    #[test]
    fn the_hour_table_adds_days_by_weekday_from_monday() {
        // 2026-03-02 is a Monday, 2026-03-08 a Sunday, 2026-03-09 the next Monday.
        let table = hour_table(
            &[
                at("2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z"),
                at("2026-03-08T09:00:00Z", "2026-03-08T10:00:00Z"),
                at("2026-03-09T09:00:00Z", "2026-03-09T09:15:00Z"),
            ],
            &utc_zone(),
        );
        assert_eq!(table[0][9], 2700.);
        assert_eq!(table[6][9], 3600.);
    }

    #[test]
    fn medians_take_the_middle_or_the_mean_of_the_middle_two() {
        assert_eq!(median([3., 1., 2.].into_iter()), 2.);
        assert_eq!(median([4., 1., 3., 2.].into_iter()), 2.5);
        assert_eq!(median(std::iter::empty()), 0.);
        assert_eq!(quantile(&[0., 10., 20., 30., 40.], 0.25), 10.);
    }

    #[test]
    fn habits_read_active_hours_typical_times_and_shares() {
        // Mon, Tue, Wed 09:00–10:00, Sat 22:00–23:00 too.
        let intervals = [
            at("2026-03-02T09:00:00Z", "2026-03-02T10:00:00Z"),
            at("2026-03-03T09:00:00Z", "2026-03-03T10:00:00Z"),
            at("2026-03-04T09:00:00Z", "2026-03-04T10:00:00Z"),
            at("2026-03-07T22:00:00Z", "2026-03-07T23:00:00Z"),
        ];
        let days = day_profiles(&intervals, &utc_zone());
        let habits = summarize(&days, &hour_table(&intervals, &utc_zone()));
        assert_eq!(habits.active_days, 4);
        assert_eq!(habits.active_hours, [HourRange { start: 9, end: 10 }]);
        assert_eq!(habits.typical_start, Some(9 * 60));
        assert_eq!(habits.typical_end, Some(10 * 60));
        assert_eq!(habits.peak_hours, [HourRange { start: 9, end: 10 }, HourRange { start: 22, end: 23 }]);
        assert_eq!(habits.late_share, 0.25);
        assert_eq!(habits.weekend_share, 0.25);
        assert_eq!(habits.median_focus_seconds, 3600.);
        assert_eq!(habits.fragmentation, 1.);
    }

    fn quiet() -> Habits {
        Habits {
            active_days: 3,
            start_quartiles: Some((540, 630)),
            late_share: 0.149,
            weekend_share: 0.199,
            longest_focus: Some(LongestFocus {
                day: "2026-03-02".into(),
                seconds: 3600.,
            }),
            median_focus_seconds: SHORT_FOCUS_SECONDS,
            ..Habits::default()
        }
    }

    #[test]
    fn nothing_crosses_a_threshold_so_nothing_is_said() {
        assert!(observations(&quiet()).is_empty());
    }

    #[test]
    fn each_threshold_raises_its_own_observation() {
        type Raise = fn(&mut Habits);
        let cases: [(Raise, &str); 5] = [
            (|h| h.start_quartiles = Some((540, 631)), "Your start time swings from 9:00 to 10:31."),
            (|h| h.late_share = 0.15, "15% of your time is late at night, after 22:00."),
            (|h| h.weekend_share = 0.2, "20% of your time falls on weekends."),
            (
                |h| h.median_focus_seconds = 18. * 60.,
                "Focus streaks are short; the longest one on a typical day is 18 min.",
            ),
            (|h| h.peak_hours = vec![HourRange { start: 9, end: 11 }], "You are most active 9–11."),
        ];
        for (raise, text) in cases {
            let mut habits = quiet();
            raise(&mut habits);
            let said = observations(&habits);
            assert_eq!(said.len(), 1, "{text}");
            assert_eq!(said[0].text, text);
            assert!(said[0].suggestion.is_some());
        }
    }

    #[test]
    fn at_most_four_observations_and_none_before_three_days() {
        let loud = Habits {
            start_quartiles: Some((480, 700)),
            late_share: 0.5,
            weekend_share: 0.4,
            median_focus_seconds: 600.,
            peak_hours: vec![HourRange { start: 9, end: 12 }],
            ..quiet()
        };
        let said = observations(&loud);
        assert_eq!(said.len(), 4);
        assert!(said.iter().all(|o| !o.text.starts_with("You are most active")));

        assert!(observations(&Habits { active_days: 2, ..loud }).is_empty());
    }

    #[test]
    fn the_timeline_merges_close_blocks_of_one_category() {
        let rows = timeline(
            &[
                tagged("2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z", Some(1)),
                tagged("2026-03-02T09:31:00Z", "2026-03-02T10:00:00Z", Some(1)),
                tagged("2026-03-02T10:00:00Z", "2026-03-02T10:30:00Z", Some(2)),
                tagged("2026-03-02T10:33:00Z", "2026-03-02T11:00:00Z", Some(2)),
                tagged("2026-03-02T23:30:00Z", "2026-03-03T00:30:00Z", None),
            ],
            &utc_zone(),
        );
        let spans = |day: &TimelineDay| -> Vec<(f64, f64, Option<CategoryId>)> {
            day.blocks.iter().map(|b| (b.start, b.end, b.category_id)).collect()
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(
            spans(&rows[0]),
            [
                (32400., 36000., Some(CategoryId(1))),
                (36000., 37800., Some(CategoryId(2))),
                (37980., 39600., Some(CategoryId(2))),
                (84600., 86400., None),
            ]
        );
        assert_eq!(spans(&rows[1]), [(0., 1800., None)]);
    }
}
