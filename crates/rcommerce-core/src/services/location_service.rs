//! Opening-hours rules for store locations: whether a location is open now,
//! when it closes or next opens, and when an order placed now is ready for
//! collection.
//!
//! Hours are stored in the location's local time; every result is a UTC
//! instant, so daylight-saving changes are handled by the time zone database.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use crate::models::location::{LocationClosure, OpeningRange};

/// How far ahead to look for the next opening (long holiday closures included)
const LOOKAHEAD_DAYS: i64 = 31;

/// Open now, and the next change
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpeningStatus {
    pub is_open: bool,
    /// When the current opening range ends (open only)
    pub closes_at: Option<DateTime<Utc>>,
    /// When the location next opens (closed only)
    pub next_opening: Option<DateTime<Utc>>,
}

/// The weekly hours and special days of one location
#[derive(Debug, Clone)]
pub struct OpeningSchedule {
    timezone: Tz,
    weekly: Vec<OpeningRange>,
    closures: Vec<LocationClosure>,
}

impl OpeningSchedule {
    /// An unknown time zone name falls back to UTC (names are validated on input)
    pub fn new(timezone: &str, weekly: Vec<OpeningRange>, closures: Vec<LocationClosure>) -> Self {
        Self {
            timezone: timezone.parse().unwrap_or(Tz::UTC),
            weekly,
            closures,
        }
    }

    /// Opening ranges of a local date: its special day if it has one (none when
    /// closed all day), otherwise the weekly hours of its weekday
    pub fn ranges_on(&self, date: NaiveDate) -> Vec<(NaiveTime, NaiveTime)> {
        let special: Vec<&LocationClosure> =
            self.closures.iter().filter(|c| c.date == date).collect();
        let mut ranges: Vec<(NaiveTime, NaiveTime)> = if special.is_empty() {
            let weekday = date.weekday().number_from_monday() as i16;
            self.weekly
                .iter()
                .filter(|r| r.weekday == weekday)
                .map(|r| (r.opens_at, r.closes_at))
                .collect()
        } else if special.iter().any(|c| c.opens_at.is_none()) {
            Vec::new()
        } else {
            special
                .iter()
                .filter_map(|c| Some((c.opens_at?, c.closes_at?)))
                .collect()
        };
        ranges.sort();
        ranges
    }

    /// Opening ranges as UTC instants, from the local day of `from` onwards,
    /// skipping ranges that have already ended
    fn upcoming(
        &self,
        from: DateTime<Utc>,
    ) -> impl Iterator<Item = (DateTime<Utc>, DateTime<Utc>)> + '_ {
        let first_day = from.with_timezone(&self.timezone).date_naive();
        (0..LOOKAHEAD_DAYS)
            .filter_map(move |offset| first_day.checked_add_signed(Duration::days(offset)))
            .flat_map(move |day| {
                self.ranges_on(day)
                    .into_iter()
                    .filter_map(move |(opens, closes)| {
                        Some((self.instant(day, opens)?, self.instant(day, closes)?))
                    })
            })
            .filter(move |(_, closes)| *closes > from)
    }

    /// A local date and time as a UTC instant. A time skipped by a
    /// daylight-saving change has no instant.
    fn instant(&self, day: NaiveDate, time: NaiveTime) -> Option<DateTime<Utc>> {
        self.timezone
            .from_local_datetime(&day.and_time(time))
            .earliest()
            .map(|t| t.with_timezone(&Utc))
    }

    /// Open at `now`, with the closing time or the next opening
    pub fn status_at(&self, now: DateTime<Utc>) -> OpeningStatus {
        match self.upcoming(now).next() {
            Some((opens, closes)) if opens <= now => OpeningStatus {
                is_open: true,
                closes_at: Some(closes),
                next_opening: None,
            },
            Some((opens, _)) => OpeningStatus {
                is_open: false,
                closes_at: None,
                next_opening: Some(opens),
            },
            None => OpeningStatus {
                is_open: false,
                closes_at: None,
                next_opening: None,
            },
        }
    }

    /// `at` itself when the location is open then, otherwise its next opening
    pub fn open_at_or_after(&self, at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.upcoming(at).next().map(|(opens, _)| opens.max(at))
    }

    /// When an order placed at `ordered_at` can be collected: after the
    /// preparation lead time, at the first moment the location is open. An
    /// order placed in the evening is therefore ready at the next opening
    /// ("order the day before, collect from 7:00").
    pub fn pickup_ready_at(
        &self,
        ordered_at: DateTime<Utc>,
        lead_minutes: i32,
    ) -> Option<DateTime<Utc>> {
        self.open_at_or_after(ordered_at + Duration::minutes(i64::from(lead_minutes.max(0))))
    }
}

/// Great-circle distance in kilometres between two coordinates
pub fn distance_km(from: (f64, f64), to: (f64, f64)) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let (lat1, lon1) = (from.0.to_radians(), from.1.to_radians());
    let (lat2, lon2) = (to.0.to_radians(), to.1.to_radians());
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn day(y: i32, mo: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap()
    }

    /// Brussels local time as UTC
    fn local(y: i32, mo: u32, d: u32, h: u32, m: u32) -> DateTime<Utc> {
        chrono_tz::Europe::Brussels
            .with_ymd_and_hms(y, mo, d, h, m, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// A typical branch: Mon-Fri 7:00-18:00, Sat 8:00-12:00, closed Sunday
    fn typical_branch(closures: Vec<LocationClosure>) -> OpeningSchedule {
        let mut weekly: Vec<OpeningRange> = (1..=5)
            .map(|weekday| OpeningRange {
                weekday,
                opens_at: t(7, 0),
                closes_at: t(18, 0),
            })
            .collect();
        weekly.push(OpeningRange {
            weekday: 6,
            opens_at: t(8, 0),
            closes_at: t(12, 0),
        });
        OpeningSchedule::new("Europe/Brussels", weekly, closures)
    }

    fn closure(date: NaiveDate, hours: Option<(NaiveTime, NaiveTime)>) -> LocationClosure {
        LocationClosure {
            id: Uuid::new_v4(),
            location_id: Uuid::nil(),
            date,
            opens_at: hours.map(|h| h.0),
            closes_at: hours.map(|h| h.1),
            reason: None,
        }
    }

    #[test]
    fn open_with_closing_time_and_closed_with_next_opening() {
        let s = typical_branch(vec![]);
        // Tuesday 27 October 2026, 10:00: open, closes at 18:00
        assert_eq!(
            s.status_at(local(2026, 10, 27, 10, 0)),
            OpeningStatus {
                is_open: true,
                closes_at: Some(local(2026, 10, 27, 18, 0)),
                next_opening: None
            }
        );
        // 18:00 is closed: next opening Wednesday 7:00
        assert_eq!(
            s.status_at(local(2026, 10, 27, 18, 0)),
            OpeningStatus {
                is_open: false,
                closes_at: None,
                next_opening: Some(local(2026, 10, 28, 7, 0))
            }
        );
        // Saturday 13:00: closed until Monday 7:00 (Sunday closed)
        assert_eq!(
            s.status_at(local(2026, 10, 31, 13, 0)).next_opening,
            Some(local(2026, 11, 2, 7, 0))
        );
    }

    #[test]
    fn special_days_replace_the_weekly_hours() {
        // 11 November (Armistice) closed; 24 December open 8:00-12:00 only
        let s = typical_branch(vec![
            closure(day(2026, 11, 11), None),
            closure(day(2026, 12, 24), Some((t(8, 0), t(12, 0)))),
        ]);
        assert!(!s.status_at(local(2026, 11, 11, 10, 0)).is_open);
        assert_eq!(
            s.status_at(local(2026, 11, 11, 10, 0)).next_opening,
            Some(local(2026, 11, 12, 7, 0))
        );
        assert!(!s.status_at(local(2026, 12, 24, 7, 30)).is_open);
        assert_eq!(
            s.status_at(local(2026, 12, 24, 9, 0)).closes_at,
            Some(local(2026, 12, 24, 12, 0))
        );
    }

    #[test]
    fn lunch_break_ranges() {
        let s = OpeningSchedule::new(
            "Europe/Brussels",
            vec![
                OpeningRange {
                    weekday: 2,
                    opens_at: t(8, 0),
                    closes_at: t(12, 0),
                },
                OpeningRange {
                    weekday: 2,
                    opens_at: t(13, 0),
                    closes_at: t(17, 0),
                },
            ],
            vec![],
        );
        let status = s.status_at(local(2026, 10, 27, 12, 30));
        assert!(!status.is_open);
        assert_eq!(status.next_opening, Some(local(2026, 10, 27, 13, 0)));
    }

    #[test]
    fn pickup_ready_within_the_hour_or_at_next_opening() {
        let s = typical_branch(vec![]);
        // Tuesday 10:15 + 60 min -> 11:15 the same day
        assert_eq!(
            s.pickup_ready_at(local(2026, 10, 27, 10, 15), 60),
            Some(local(2026, 10, 27, 11, 15))
        );
        // Tuesday 17:30 + 60 min is after closing -> Wednesday 7:00
        assert_eq!(
            s.pickup_ready_at(local(2026, 10, 27, 17, 30), 60),
            Some(local(2026, 10, 28, 7, 0))
        );
        // Ordered the evening before -> ready from opening
        assert_eq!(
            s.pickup_ready_at(local(2026, 10, 27, 21, 0), 60),
            Some(local(2026, 10, 28, 7, 0))
        );
        // Saturday 11:30 + 60 -> Monday 7:00
        assert_eq!(
            s.pickup_ready_at(local(2026, 10, 31, 11, 30), 60),
            Some(local(2026, 11, 2, 7, 0))
        );
    }

    #[test]
    fn daylight_saving_change_keeps_local_hours() {
        // Brussels leaves summer time on Sunday 25 October 2026: Monday's
        // 7:00 opening is 06:00 UTC (it was 05:00 UTC the Monday before)
        let s = typical_branch(vec![]);
        let monday = s
            .status_at(local(2026, 10, 25, 12, 0))
            .next_opening
            .unwrap();
        assert_eq!(monday, Utc.with_ymd_and_hms(2026, 10, 26, 6, 0, 0).unwrap());
        let week_before = s
            .status_at(local(2026, 10, 18, 12, 0))
            .next_opening
            .unwrap();
        assert_eq!(
            week_before,
            Utc.with_ymd_and_hms(2026, 10, 19, 5, 0, 0).unwrap()
        );
    }

    #[test]
    fn never_open_has_no_next_opening() {
        let s = OpeningSchedule::new("Europe/Brussels", vec![], vec![]);
        assert_eq!(
            s.status_at(local(2026, 10, 27, 10, 0)),
            OpeningStatus {
                is_open: false,
                closes_at: None,
                next_opening: None
            }
        );
        assert_eq!(s.pickup_ready_at(local(2026, 10, 27, 10, 0), 60), None);
    }

    #[test]
    fn distance_between_brussels_and_antwerp() {
        // Brussels to Antwerp: about 41 km
        let d = distance_km((50.8467, 4.3525), (51.2194, 4.4025));
        assert!((38.0..44.0).contains(&d), "{d}");
        assert!(distance_km((50.0, 4.0), (50.0, 4.0)) < 1e-9);
    }
}
