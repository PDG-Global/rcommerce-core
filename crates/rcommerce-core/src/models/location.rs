//! Store locations: branches and pickup points shown on the storefront, with
//! their opening hours and special days. The schedule rules (open now, next
//! opening, ready-for-collection time) are in `services::location_service`.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::common::validation::validate_slug;
use crate::{Error, Result};

/// An inventory location with its storefront details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StoreLocation {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub slug: Option<String>,
    pub is_active: bool,
    /// Listed on the storefront
    pub is_public: bool,
    /// Customers can collect orders here
    pub is_pickup_point: bool,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// IANA time zone of the opening hours, e.g. "Europe/Brussels"
    pub timezone: String,
    /// Minutes needed to prepare an order for collection
    pub pickup_lead_minutes: i32,
    /// Services offered, e.g. "pickup", "delivery", "showroom", "timber_cutting"
    pub services: Vec<String>,
    pub manager_name: Option<String>,
    pub sort_order: i32,
    /// Translated in the requested locale (None for the base text)
    #[sqlx(default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub pickup_instructions: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One opening range in the location's local time. A day can have several
/// (closed at lunch). ISO weekday: 1 = Monday ... 7 = Sunday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct OpeningRange {
    pub weekday: i16,
    pub opens_at: NaiveTime,
    pub closes_at: NaiveTime,
}

/// A day that differs from the weekly hours: closed when both times are None,
/// otherwise open only during the given range (several rows for several ranges)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct LocationClosure {
    pub id: Uuid,
    pub location_id: Uuid,
    pub date: NaiveDate,
    pub opens_at: Option<NaiveTime>,
    pub closes_at: Option<NaiveTime>,
    pub reason: Option<String>,
}

/// Create or replace a location's details (admin)
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LocationInput {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[validate(length(min = 1, max = 50))]
    pub code: String,
    pub slug: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default)]
    pub is_pickup_point: bool,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    #[validate(email)]
    pub email: Option<String>,
    #[validate(length(max = 255))]
    pub address_line1: Option<String>,
    #[validate(length(max = 255))]
    pub address_line2: Option<String>,
    #[validate(length(max = 20))]
    pub postal_code: Option<String>,
    #[validate(length(max = 100))]
    pub city: Option<String>,
    pub country_code: Option<String>,
    #[validate(range(min = -90.0, max = 90.0))]
    pub latitude: Option<f64>,
    #[validate(range(min = -180.0, max = 180.0))]
    pub longitude: Option<f64>,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// At most one week
    #[serde(default = "default_pickup_lead")]
    #[validate(range(min = 0, max = 10080))]
    pub pickup_lead_minutes: i32,
    #[serde(default)]
    pub services: Vec<String>,
    #[validate(length(max = 255))]
    pub manager_name: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_true() -> bool {
    true
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_pickup_lead() -> i32 {
    60
}

impl LocationInput {
    /// Field rules beyond the derive: slug shape, time zone name, country code
    /// (ISO 3166 alpha-2, upper case) and service names
    pub fn check(&self) -> Result<()> {
        self.validate()
            .map_err(|e| Error::validation(format!("Invalid location: {e}")))?;
        if let Some(slug) = &self.slug {
            if !validate_slug(slug) {
                return Err(Error::validation(
                    "Slug must be lower-case letters and digits separated by hyphens",
                ));
            }
        }
        if self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(Error::validation(
                "Timezone must be an IANA name such as Europe/Brussels",
            ));
        }
        if let Some(country) = &self.country_code {
            if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_uppercase()) {
                return Err(Error::validation(
                    "Country code must be two upper-case letters (ISO 3166)",
                ));
            }
        }
        if self.services.len() > 30
            || !self.services.iter().all(|s| {
                (1..=50).contains(&s.len())
                    && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            })
        {
            return Err(Error::validation(
                "Services must be at most 30 lower-case names such as \"timber_cutting\"",
            ));
        }
        Ok(())
    }
}

/// Replace the weekly opening hours (admin)
#[derive(Debug, Clone, Deserialize)]
pub struct OpeningHoursInput {
    pub ranges: Vec<OpeningRange>,
}

impl OpeningHoursInput {
    /// Weekdays 1-7, each range closing after it opens, no overlap within a day
    pub fn check(&self) -> Result<()> {
        if self.ranges.len() > 50 {
            return Err(Error::validation("At most 50 opening ranges"));
        }
        let mut sorted = self.ranges.clone();
        sorted.sort_by_key(|r| (r.weekday, r.opens_at));
        for (i, range) in sorted.iter().enumerate() {
            if !(1..=7).contains(&range.weekday) {
                return Err(Error::validation(
                    "Weekday must be 1 (Monday) to 7 (Sunday)",
                ));
            }
            if range.closes_at <= range.opens_at {
                return Err(Error::validation(
                    "Each opening range must close after it opens",
                ));
            }
            if let Some(next) = sorted.get(i + 1) {
                if next.weekday == range.weekday && next.opens_at < range.closes_at {
                    return Err(Error::validation(
                        "Opening ranges of the same day must not overlap",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Add a special day (admin)
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ClosureInput {
    pub date: NaiveDate,
    pub opens_at: Option<NaiveTime>,
    pub closes_at: Option<NaiveTime>,
    #[validate(length(max = 255))]
    pub reason: Option<String>,
}

impl ClosureInput {
    /// Closed all day (no times) or one range closing after it opens
    pub fn check(&self) -> Result<()> {
        self.validate()
            .map_err(|e| Error::validation(format!("Invalid special day: {e}")))?;
        match (self.opens_at, self.closes_at) {
            (None, None) => Ok(()),
            (Some(opens), Some(closes)) if closes > opens => Ok(()),
            _ => Err(Error::validation(
                "Give both opening and closing times, closing after opening, or neither for a closed day",
            )),
        }
    }
}

/// Translated text of a location (admin)
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LocationTranslationInput {
    #[validate(length(max = 5000))]
    pub description: Option<String>,
    #[validate(length(max = 2000))]
    pub pickup_instructions: Option<String>,
}

impl LocationTranslationInput {
    /// Length limits
    pub fn check(&self) -> Result<()> {
        self.validate()
            .map_err(|e| Error::validation(format!("Invalid translation: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> LocationInput {
        serde_json::from_value(serde_json::json!({
            "name": "Central",
            "code": "CEN",
            "slug": "central",
            "is_public": true,
            "is_pickup_point": true,
            "country_code": "BE",
            "timezone": "Europe/Brussels",
            "services": ["pickup", "delivery", "showroom"]
        }))
        .unwrap()
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn location_input_defaults_and_rules() {
        let ok = input();
        assert!(ok.check().is_ok());
        assert!(ok.is_active);
        assert_eq!(ok.pickup_lead_minutes, 60);

        for (field, value) in [
            ("slug", serde_json::json!("Central")),
            ("timezone", serde_json::json!("Brussels")),
            ("country_code", serde_json::json!("be")),
            ("services", serde_json::json!(["Pickup"])),
            ("pickup_lead_minutes", serde_json::json!(-1)),
            ("latitude", serde_json::json!(91.0)),
        ] {
            let mut json = serde_json::to_value(input_json()).unwrap();
            json[field] = value;
            let bad: LocationInput = serde_json::from_value(json).unwrap();
            assert!(bad.check().is_err(), "{field} should be rejected");
        }
    }

    fn input_json() -> serde_json::Value {
        serde_json::json!({
            "name": "Central", "code": "CEN", "slug": "central",
            "country_code": "BE", "timezone": "Europe/Brussels", "services": ["pickup"]
        })
    }

    #[test]
    fn opening_hours_must_be_ordered_and_not_overlap() {
        let range = |weekday, opens, closes| OpeningRange {
            weekday,
            opens_at: opens,
            closes_at: closes,
        };
        let lunch_break = OpeningHoursInput {
            ranges: vec![
                range(1, time(13, 0), time(18, 0)),
                range(1, time(7, 0), time(12, 0)),
            ],
        };
        assert!(lunch_break.check().is_ok());

        let overlap = OpeningHoursInput {
            ranges: vec![
                range(1, time(7, 0), time(12, 30)),
                range(1, time(12, 0), time(18, 0)),
            ],
        };
        assert!(overlap.check().is_err());
        let backwards = OpeningHoursInput {
            ranges: vec![range(2, time(18, 0), time(7, 0))],
        };
        assert!(backwards.check().is_err());
        let sunday_plus_one = OpeningHoursInput {
            ranges: vec![range(8, time(7, 0), time(12, 0))],
        };
        assert!(sunday_plus_one.check().is_err());
    }

    #[test]
    fn closures_are_all_day_or_one_range() {
        let day = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap();
        let closed = ClosureInput {
            date: day,
            opens_at: None,
            closes_at: None,
            reason: Some("Noël".into()),
        };
        assert!(closed.check().is_ok());
        let short_day = ClosureInput {
            opens_at: Some(time(8, 0)),
            closes_at: Some(time(12, 0)),
            ..closed.clone()
        };
        assert!(short_day.check().is_ok());
        let half = ClosureInput {
            opens_at: Some(time(8, 0)),
            ..closed
        };
        assert!(half.check().is_err());
    }
}
