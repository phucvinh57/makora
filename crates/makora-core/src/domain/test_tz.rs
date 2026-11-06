use chrono::{DateTime, FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};

pub fn utc(at: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(at).unwrap().with_timezone(&Utc)
}

/// UTC+1 that moves to UTC+2 at local midnight going into 2026-03-29, so that day starts at
/// 01:00 local (2026-03-28T23:00Z) and is 23 hours long.
#[derive(Clone)]
pub struct MidnightDst;

const SWITCH_UTC: &str = "2026-03-28T23:00:00Z";

impl MidnightDst {
    fn offset_at(utc_at: DateTime<Utc>) -> FixedOffset {
        let hours = if utc_at < utc(SWITCH_UTC) { 1 } else { 2 };
        FixedOffset::east_opt(hours * 3600).unwrap()
    }
}

impl TimeZone for MidnightDst {
    type Offset = FixedOffset;

    fn from_offset(_: &FixedOffset) -> Self {
        MidnightDst
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
        self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
        let before = FixedOffset::east_opt(3600).unwrap();
        let after = FixedOffset::east_opt(2 * 3600).unwrap();
        let gap_start = utc(SWITCH_UTC).naive_utc() + TimeDelta::hours(1);
        let gap_end = gap_start + TimeDelta::hours(1);
        if *local < gap_start {
            MappedLocalTime::Single(before)
        } else if *local < gap_end {
            MappedLocalTime::None
        } else {
            MappedLocalTime::Single(after)
        }
    }

    fn offset_from_utc_date(&self, utc_date: &NaiveDate) -> FixedOffset {
        self.offset_from_utc_datetime(&utc_date.and_time(NaiveTime::MIN))
    }

    fn offset_from_utc_datetime(&self, utc_at: &NaiveDateTime) -> FixedOffset {
        Self::offset_at(utc_at.and_utc())
    }
}
