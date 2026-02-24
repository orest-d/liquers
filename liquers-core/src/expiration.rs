use crate::error::Error;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveTime, Offset, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;

/// Specification of when an asset should expire, relative to current time.
/// Serializes/deserializes as a human-readable string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expires {
    /// Asset never expires (default)
    Never,
    /// Asset expires immediately (implies volatile)
    Immediately,
    /// Asset expires after a duration from when it becomes Ready
    InDuration(std::time::Duration),
    /// Asset expires at a specific time of day (UTC or with timezone)
    AtTimeOfDay {
        hour: u32,
        minute: u32,
        second: u32,
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires on a specific day of week at 00:00
    OnDayOfWeek {
        /// 0=Monday, 6=Sunday (chrono::Weekday convention)
        day: u32,
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at the end of the current day (next 00:00)
    EndOfDay {
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at end of week (next Monday 00:00)
    EndOfWeek {
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at end of month (1st of next month 00:00)
    EndOfMonth {
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at a specific date and time (always UTC after parsing)
    AtDateTime(DateTime<Utc>),
}

impl Default for Expires {
    fn default() -> Self {
        Expires::Never
    }
}

impl Expires {
    /// Compute ExpirationTime from Expires specification at the given reference time.
    /// Reference time is typically when the asset becomes Ready.
    /// tz_offset_default is the system timezone offset in seconds if not specified in the Expires spec.
    pub fn to_expiration_time(
        &self,
        reference_time: DateTime<Utc>,
        tz_offset_default: i32,
    ) -> ExpirationTime {
        match self {
            Expires::Never => ExpirationTime::Never,
            Expires::Immediately => ExpirationTime::Immediately,
            Expires::InDuration(d) => {
                let chrono_dur = Duration::from_std(*d).unwrap_or(Duration::zero());
                ExpirationTime::At(reference_time + chrono_dur)
            }
            Expires::AtTimeOfDay {
                hour,
                minute,
                second,
                tz_offset,
            } => {
                let offset_secs = tz_offset.unwrap_or(tz_offset_default);
                let tz = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap_or(Utc.fix()));
                let local_now = reference_time.with_timezone(&tz);
                let target_time =
                    NaiveTime::from_hms_opt(*hour, *minute, *second).unwrap_or(NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default());
                let today_target = local_now.date_naive().and_time(target_time);
                let today_target_utc = tz
                    .from_local_datetime(&today_target)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(reference_time);
                if today_target_utc > reference_time {
                    ExpirationTime::At(today_target_utc)
                } else {
                    // Next day
                    let tomorrow_target = today_target + Duration::days(1);
                    let tomorrow_utc = tz
                        .from_local_datetime(&tomorrow_target)
                        .single()
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(reference_time + Duration::days(1));
                    ExpirationTime::At(tomorrow_utc)
                }
            }
            Expires::OnDayOfWeek { day, tz_offset } => {
                let offset_secs = tz_offset.unwrap_or(tz_offset_default);
                let tz = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap_or(Utc.fix()));
                let local_now = reference_time.with_timezone(&tz);
                let current_weekday = local_now.weekday().num_days_from_monday();
                let target_day = *day % 7;
                let days_ahead = if target_day > current_weekday {
                    target_day - current_weekday
                } else if target_day < current_weekday {
                    7 - (current_weekday - target_day)
                } else {
                    7 // Same day means next week
                };
                let target_date = local_now.date_naive() + Duration::days(days_ahead as i64);
                let target_naive = target_date.and_hms_opt(0, 0, 0).unwrap_or_default();
                let target_utc = tz
                    .from_local_datetime(&target_naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(reference_time + Duration::days(days_ahead as i64));
                ExpirationTime::At(target_utc)
            }
            Expires::EndOfDay { tz_offset } => {
                let offset_secs = tz_offset.unwrap_or(tz_offset_default);
                let tz = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap_or(Utc.fix()));
                let local_now = reference_time.with_timezone(&tz);
                let tomorrow = local_now.date_naive() + Duration::days(1);
                let target_naive = tomorrow.and_hms_opt(0, 0, 0).unwrap_or_default();
                let target_utc = tz
                    .from_local_datetime(&target_naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(reference_time + Duration::days(1));
                ExpirationTime::At(target_utc)
            }
            Expires::EndOfWeek { tz_offset } => {
                let offset_secs = tz_offset.unwrap_or(tz_offset_default);
                let tz = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap_or(Utc.fix()));
                let local_now = reference_time.with_timezone(&tz);
                let current_weekday = local_now.weekday().num_days_from_monday();
                let days_to_monday = if current_weekday == 0 { 7 } else { 7 - current_weekday };
                let next_monday = local_now.date_naive() + Duration::days(days_to_monday as i64);
                let target_naive = next_monday.and_hms_opt(0, 0, 0).unwrap_or_default();
                let target_utc = tz
                    .from_local_datetime(&target_naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(reference_time + Duration::days(days_to_monday as i64));
                ExpirationTime::At(target_utc)
            }
            Expires::EndOfMonth { tz_offset } => {
                let offset_secs = tz_offset.unwrap_or(tz_offset_default);
                let tz = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap_or(Utc.fix()));
                let local_now = reference_time.with_timezone(&tz);
                let (year, month) = (local_now.year(), local_now.month());
                let (next_year, next_month) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
                let first_of_next = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
                    .unwrap_or(local_now.date_naive() + Duration::days(30));
                let target_naive = first_of_next.and_hms_opt(0, 0, 0).unwrap_or_default();
                let target_utc = tz
                    .from_local_datetime(&target_naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(reference_time + Duration::days(30));
                ExpirationTime::At(target_utc)
            }
            Expires::AtDateTime(dt) => ExpirationTime::At(*dt),
        }
    }

    /// Returns true if this Expires implies volatile semantics
    pub fn is_volatile(&self) -> bool {
        matches!(self, Expires::Immediately)
    }

    /// Returns true if this is Never
    pub fn is_never(&self) -> bool {
        matches!(self, Expires::Never)
    }
}

// --- Timezone abbreviation lookup ---

fn tz_abbreviation_to_offset(abbr: &str) -> Option<i32> {
    match abbr.to_uppercase().as_str() {
        "UTC" | "GMT" => Some(0),
        "EST" => Some(-5 * 3600),
        "EDT" => Some(-4 * 3600),
        "CST" => Some(-6 * 3600),
        "CDT" => Some(-5 * 3600),
        "MST" => Some(-7 * 3600),
        "MDT" => Some(-6 * 3600),
        "PST" => Some(-8 * 3600),
        "PDT" => Some(-7 * 3600),
        "CET" => Some(1 * 3600),
        "CEST" => Some(2 * 3600),
        "EET" => Some(2 * 3600),
        "EEST" => Some(3 * 3600),
        _ => None,
    }
}

fn day_name_to_number(name: &str) -> Option<u32> {
    match name.to_lowercase().as_str() {
        "monday" | "mon" => Some(0),
        "tuesday" | "tue" | "tues" => Some(1),
        "wednesday" | "wed" => Some(2),
        "thursday" | "thu" | "thur" | "thurs" => Some(3),
        "friday" | "fri" => Some(4),
        "saturday" | "sat" => Some(5),
        "sunday" | "sun" => Some(6),
        _ => None,
    }
}

/// Normalize a duration to the largest unit that divides evenly.
fn format_duration(d: &std::time::Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms == 0 {
        return "in 0 ms".to_string();
    }
    let total_secs = d.as_secs();

    // Check from largest to smallest unit
    if total_secs > 0 && total_secs % (30 * 86400) == 0 && total_ms % (30 * 86400 * 1000) == 0 {
        let months = total_secs / (30 * 86400);
        return format!("in {} {}", months, if months == 1 { "month" } else { "months" });
    }
    if total_secs > 0 && total_secs % (7 * 86400) == 0 {
        let weeks = total_secs / (7 * 86400);
        return format!("in {} {}", weeks, if weeks == 1 { "week" } else { "weeks" });
    }
    if total_secs > 0 && total_secs % 86400 == 0 {
        let days = total_secs / 86400;
        return format!("in {} {}", days, if days == 1 { "day" } else { "days" });
    }
    if total_secs > 0 && total_secs % 3600 == 0 {
        let hours = total_secs / 3600;
        return format!("in {} {}", hours, if hours == 1 { "hour" } else { "hours" });
    }
    if total_secs > 0 && total_secs % 60 == 0 {
        let minutes = total_secs / 60;
        return format!("in {} min", minutes);
    }
    if total_secs > 0 && total_ms % 1000 == 0 {
        return format!("in {} {}", total_secs, if total_secs == 1 { "second" } else { "seconds" });
    }
    format!("in {} ms", total_ms)
}

fn weekday_name(day: u32) -> &'static str {
    match day % 7 {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        6 => "Sunday",
        _ => "Monday", // unreachable due to % 7
    }
}

// --- FromStr for Expires ---

impl std::str::FromStr for Expires {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(Error::general_error(
                "Empty expiration specification".to_string(),
            ));
        }
        let lower = trimmed.to_lowercase();

        // Keywords
        if lower == "never" {
            return Ok(Expires::Never);
        }
        if lower == "immediately" {
            return Ok(Expires::Immediately);
        }

        // End-of-period aliases
        if lower == "eod" || lower == "end of day" {
            return Ok(Expires::EndOfDay { tz_offset: None });
        }
        if lower == "end of week" || lower == "eow" {
            return Ok(Expires::EndOfWeek { tz_offset: None });
        }
        if lower == "end of month" || lower == "eom" {
            return Ok(Expires::EndOfMonth { tz_offset: None });
        }

        // "on <DayOfWeek>"
        if let Some(rest) = lower.strip_prefix("on ") {
            let rest = rest.trim();
            if let Some(day) = day_name_to_number(rest) {
                return Ok(Expires::OnDayOfWeek {
                    day,
                    tz_offset: None,
                });
            }
            return Err(Error::general_error(format!(
                "Invalid day of week: '{}'",
                rest
            )));
        }

        // "at HH:MM[:SS] [TZ]"
        if let Some(rest) = lower.strip_prefix("at ") {
            return parse_time_of_day(rest.trim());
        }

        // Duration: "in X unit" or "X unit"
        let duration_str = if let Some(rest) = lower.strip_prefix("in ") {
            rest.trim()
        } else {
            &lower
        };

        if let Ok(expires) = parse_duration(duration_str) {
            return Ok(expires);
        }

        // Try absolute date/time parsing
        // ISO 8601: "2026-03-01", "2026-03-01 15:00", "2026-03-01T15:00:00Z"
        if let Ok(dt) = trimmed.parse::<DateTime<Utc>>() {
            return Ok(Expires::AtDateTime(dt));
        }
        // Try with chrono's flexible parsing
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
            return Ok(Expires::AtDateTime(dt.and_utc()));
        }
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return Ok(Expires::AtDateTime(dt.and_utc()));
        }
        if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
            let dt = d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc();
            return Ok(Expires::AtDateTime(dt));
        }

        Err(Error::general_error(format!(
            "Invalid expiration specification: '{}'",
            trimmed
        )))
    }
}

fn parse_time_of_day(s: &str) -> Result<Expires, Error> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return Err(Error::general_error(
            "Invalid time of day specification".to_string(),
        ));
    }
    let time_str = parts[0];
    let tz_offset = if parts.len() > 1 {
        let tz_str = parts[1];
        tz_abbreviation_to_offset(tz_str).map(Some).unwrap_or_else(|| {
            // Try parsing as +HH:MM or -HH:MM offset
            None
        })
    } else {
        None
    };

    // Parse HH:MM or HH:MM:SS
    let time_parts: Vec<&str> = time_str.split(':').collect();
    if time_parts.len() < 2 || time_parts.len() > 3 {
        return Err(Error::general_error(format!(
            "Invalid time format: '{}'",
            time_str
        )));
    }
    let hour: u32 = time_parts[0]
        .parse()
        .map_err(|_| Error::general_error(format!("Invalid hour: '{}'", time_parts[0])))?;
    let minute: u32 = time_parts[1]
        .parse()
        .map_err(|_| Error::general_error(format!("Invalid minute: '{}'", time_parts[1])))?;
    let second: u32 = if time_parts.len() == 3 {
        time_parts[2]
            .parse()
            .map_err(|_| Error::general_error(format!("Invalid second: '{}'", time_parts[2])))?
    } else {
        0
    };

    if hour > 23 || minute > 59 || second > 59 {
        return Err(Error::general_error(format!(
            "Time out of range: '{}'",
            time_str
        )));
    }

    Ok(Expires::AtTimeOfDay {
        hour,
        minute,
        second,
        tz_offset,
    })
}

fn parse_duration(s: &str) -> Result<Expires, Error> {
    // Parse "<number> <unit>"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        // Also try without space: "5min", "500ms"
        return parse_duration_compact(s);
    }
    let number: u64 = parts[0]
        .parse()
        .map_err(|_| Error::general_error(format!("Invalid duration number: '{}'", parts[0])))?;
    let unit = parts[1].to_lowercase();
    duration_from_number_and_unit(number, &unit)
}

fn parse_duration_compact(s: &str) -> Result<Expires, Error> {
    // Try to split at the boundary between digits and letters
    let digit_end = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| Error::general_error(format!("Invalid duration: '{}'", s)))?;
    let (num_str, unit_str) = s.split_at(digit_end);
    if num_str.is_empty() {
        return Err(Error::general_error(format!("Invalid duration: '{}'", s)));
    }
    let number: u64 = num_str
        .parse()
        .map_err(|_| Error::general_error(format!("Invalid duration number: '{}'", num_str)))?;
    let unit = unit_str.trim().to_lowercase();
    duration_from_number_and_unit(number, &unit)
}

fn duration_from_number_and_unit(number: u64, unit: &str) -> Result<Expires, Error> {
    let secs = match unit {
        "ms" | "millisecond" | "milliseconds" => {
            return Ok(Expires::InDuration(std::time::Duration::from_millis(number)));
        }
        "s" | "sec" | "second" | "seconds" => number,
        "min" | "minute" | "minutes" => number * 60,
        "h" | "hr" | "hour" | "hours" => number * 3600,
        "d" | "day" | "days" => number * 86400,
        "w" | "week" | "weeks" => number * 7 * 86400,
        "mo" | "month" | "months" => number * 30 * 86400,
        _ => {
            return Err(Error::general_error(format!(
                "Invalid duration unit: '{}'",
                unit
            )));
        }
    };
    Ok(Expires::InDuration(std::time::Duration::from_secs(secs)))
}

// --- Display for Expires ---

impl std::fmt::Display for Expires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expires::Never => write!(f, "never"),
            Expires::Immediately => write!(f, "immediately"),
            Expires::InDuration(d) => write!(f, "{}", format_duration(d)),
            Expires::AtTimeOfDay {
                hour,
                minute,
                second,
                tz_offset,
            } => {
                if *second == 0 {
                    write!(f, "at {:02}:{:02}", hour, minute)?;
                } else {
                    write!(f, "at {:02}:{:02}:{:02}", hour, minute, second)?;
                }
                if let Some(offset) = tz_offset {
                    if *offset == 0 {
                        write!(f, " UTC")?;
                    } else {
                        let h = offset / 3600;
                        let m = (offset.abs() % 3600) / 60;
                        if m == 0 {
                            write!(f, " {:+03}:00", h)?;
                        } else {
                            write!(f, " {:+03}:{:02}", h, m)?;
                        }
                    }
                }
                Ok(())
            }
            Expires::OnDayOfWeek { day, tz_offset: _ } => {
                write!(f, "on {}", weekday_name(*day))
            }
            Expires::EndOfDay { tz_offset: _ } => write!(f, "end of day"),
            Expires::EndOfWeek { tz_offset: _ } => write!(f, "end of week"),
            Expires::EndOfMonth { tz_offset: _ } => write!(f, "end of month"),
            Expires::AtDateTime(dt) => write!(f, "{}", dt.to_rfc3339()),
        }
    }
}

// --- Serde for Expires (string-based) ---

impl Serialize for Expires {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Expires {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// --- Display for ExpirationTime ---

impl std::fmt::Display for ExpirationTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpirationTime::Never => write!(f, "never"),
            ExpirationTime::Immediately => write!(f, "immediately"),
            ExpirationTime::At(dt) => write!(f, "{}", dt.to_rfc3339()),
        }
    }
}

// --- Serde for ExpirationTime (string-based) ---

impl Serialize for ExpirationTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ExpirationTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "never" => Ok(ExpirationTime::Never),
            "immediately" => Ok(ExpirationTime::Immediately),
            _ => {
                let dt: DateTime<Utc> = s
                    .parse()
                    .map_err(|e| serde::de::Error::custom(format!("Invalid expiration time: {}", e)))?;
                Ok(ExpirationTime::At(dt))
            }
        }
    }
}

/// Computed absolute expiration timestamp, always in UTC.
/// This is the resolved form of Expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpirationTime {
    /// Asset never expires
    Never,
    /// Asset expires immediately (volatile)
    Immediately,
    /// Asset expires at a specific UTC timestamp
    At(DateTime<Utc>),
}

impl Default for ExpirationTime {
    fn default() -> Self {
        ExpirationTime::Never
    }
}

impl Ord for ExpirationTime {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (ExpirationTime::Immediately, ExpirationTime::Immediately) => Ordering::Equal,
            (ExpirationTime::Immediately, _) => Ordering::Less,
            (_, ExpirationTime::Immediately) => Ordering::Greater,
            (ExpirationTime::Never, ExpirationTime::Never) => Ordering::Equal,
            (ExpirationTime::Never, _) => Ordering::Greater,
            (_, ExpirationTime::Never) => Ordering::Less,
            (ExpirationTime::At(a), ExpirationTime::At(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for ExpirationTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl ExpirationTime {
    /// Returns true if the asset has expired at the given time
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        match self {
            ExpirationTime::Never => false,
            ExpirationTime::Immediately => true,
            ExpirationTime::At(dt) => now >= *dt,
        }
    }

    /// Returns true if the asset has expired now
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now())
    }

    /// Returns true if this is Never
    pub fn is_never(&self) -> bool {
        matches!(self, ExpirationTime::Never)
    }

    /// Returns true if this is Immediately
    pub fn is_immediately(&self) -> bool {
        matches!(self, ExpirationTime::Immediately)
    }

    /// Returns the minimum of two expiration times.
    /// Used for dependency-based inference: min(self, dependency) = earliest expiration.
    pub fn min(self, other: ExpirationTime) -> ExpirationTime {
        if self <= other {
            self
        } else {
            other
        }
    }

    /// If expiration is in the past or within min_future duration, adjust to now + min_future.
    /// Used when asset transitions to Ready to ensure at least 500ms before expiration.
    pub fn ensure_future(&self, min_future: std::time::Duration) -> ExpirationTime {
        match self {
            ExpirationTime::Never => ExpirationTime::Never,
            ExpirationTime::Immediately => ExpirationTime::Immediately,
            ExpirationTime::At(dt) => {
                let now = Utc::now();
                let min_dur = Duration::from_std(min_future).unwrap_or(Duration::milliseconds(500));
                let earliest = now + min_dur;
                if *dt < earliest {
                    ExpirationTime::At(earliest)
                } else {
                    ExpirationTime::At(*dt)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    // --- Expires basics ---

    #[test]
    fn test_expires_default_is_never() {
        assert_eq!(Expires::default(), Expires::Never);
    }

    #[test]
    fn test_expires_is_volatile() {
        assert!(Expires::Immediately.is_volatile());
        assert!(!Expires::Never.is_volatile());
        assert!(!Expires::InDuration(std::time::Duration::from_secs(60)).is_volatile());
        assert!(!Expires::EndOfDay { tz_offset: None }.is_volatile());
    }

    #[test]
    fn test_expires_is_never() {
        assert!(Expires::Never.is_never());
        assert!(!Expires::Immediately.is_never());
        assert!(!Expires::InDuration(std::time::Duration::from_secs(60)).is_never());
    }

    // --- ExpirationTime basics ---

    #[test]
    fn test_expiration_time_default_is_never() {
        assert_eq!(ExpirationTime::default(), ExpirationTime::Never);
    }

    #[test]
    fn test_expiration_time_is_never() {
        assert!(ExpirationTime::Never.is_never());
        assert!(!ExpirationTime::Immediately.is_never());
        assert!(!ExpirationTime::At(Utc::now()).is_never());
    }

    #[test]
    fn test_expiration_time_is_immediately() {
        assert!(ExpirationTime::Immediately.is_immediately());
        assert!(!ExpirationTime::Never.is_immediately());
        assert!(!ExpirationTime::At(Utc::now()).is_immediately());
    }

    // --- Ordering ---

    #[test]
    fn test_ordering_immediately_less_than_at() {
        let imm = ExpirationTime::Immediately;
        let at = ExpirationTime::At(Utc::now());
        assert!(imm < at);
    }

    #[test]
    fn test_ordering_at_less_than_never() {
        let at = ExpirationTime::At(Utc::now());
        let never = ExpirationTime::Never;
        assert!(at < never);
    }

    #[test]
    fn test_ordering_immediately_less_than_never() {
        assert!(ExpirationTime::Immediately < ExpirationTime::Never);
    }

    #[test]
    fn test_ordering_at_times() {
        let early = ExpirationTime::At(Utc::now());
        let late = ExpirationTime::At(Utc::now() + Duration::hours(1));
        assert!(early < late);
    }

    #[test]
    fn test_ordering_equal_variants() {
        assert_eq!(
            ExpirationTime::Never.cmp(&ExpirationTime::Never),
            Ordering::Equal
        );
        assert_eq!(
            ExpirationTime::Immediately.cmp(&ExpirationTime::Immediately),
            Ordering::Equal
        );
    }

    // --- min ---

    #[test]
    fn test_min_immediately_and_never() {
        let result = ExpirationTime::Immediately.min(ExpirationTime::Never);
        assert_eq!(result, ExpirationTime::Immediately);
    }

    #[test]
    fn test_min_never_and_at() {
        let at = ExpirationTime::At(Utc::now());
        let result = ExpirationTime::Never.min(at.clone());
        assert_eq!(result, at);
    }

    #[test]
    fn test_min_two_at_values() {
        let early = ExpirationTime::At(Utc::now());
        let late = ExpirationTime::At(Utc::now() + Duration::hours(1));
        let result = late.min(early.clone());
        assert_eq!(result, early);
    }

    // --- is_expired ---

    #[test]
    fn test_never_not_expired() {
        assert!(!ExpirationTime::Never.is_expired());
        assert!(!ExpirationTime::Never.is_expired_at(Utc::now()));
    }

    #[test]
    fn test_immediately_always_expired() {
        assert!(ExpirationTime::Immediately.is_expired());
        assert!(ExpirationTime::Immediately.is_expired_at(Utc::now()));
    }

    #[test]
    fn test_at_past_is_expired() {
        let past = Utc::now() - Duration::hours(1);
        assert!(ExpirationTime::At(past).is_expired());
    }

    #[test]
    fn test_at_future_not_expired() {
        let future = Utc::now() + Duration::hours(1);
        assert!(!ExpirationTime::At(future).is_expired());
    }

    #[test]
    fn test_is_expired_at_specific_time() {
        let target = Utc::now() + Duration::hours(1);
        let et = ExpirationTime::At(target);
        assert!(!et.is_expired_at(target - Duration::seconds(1)));
        assert!(et.is_expired_at(target));
        assert!(et.is_expired_at(target + Duration::seconds(1)));
    }

    // --- ensure_future ---

    #[test]
    fn test_ensure_future_never_unchanged() {
        let result = ExpirationTime::Never.ensure_future(std::time::Duration::from_millis(500));
        assert_eq!(result, ExpirationTime::Never);
    }

    #[test]
    fn test_ensure_future_immediately_unchanged() {
        let result =
            ExpirationTime::Immediately.ensure_future(std::time::Duration::from_millis(500));
        assert_eq!(result, ExpirationTime::Immediately);
    }

    #[test]
    fn test_ensure_future_adjusts_past() {
        let past = Utc::now() - Duration::seconds(10);
        let et = ExpirationTime::At(past);
        let adjusted = et.ensure_future(std::time::Duration::from_millis(500));
        if let ExpirationTime::At(dt) = adjusted {
            assert!(dt > Utc::now());
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    #[test]
    fn test_ensure_future_keeps_far_future() {
        let far_future = Utc::now() + Duration::hours(1);
        let et = ExpirationTime::At(far_future);
        let result = et.ensure_future(std::time::Duration::from_millis(500));
        assert_eq!(result, ExpirationTime::At(far_future));
    }

    // --- Conversion: Expires -> ExpirationTime ---

    #[test]
    fn test_never_to_expiration_time() {
        let et = Expires::Never.to_expiration_time(Utc::now(), 0);
        assert_eq!(et, ExpirationTime::Never);
    }

    #[test]
    fn test_immediately_to_expiration_time() {
        let et = Expires::Immediately.to_expiration_time(Utc::now(), 0);
        assert_eq!(et, ExpirationTime::Immediately);
    }

    #[test]
    fn test_in_duration_to_expiration_time() {
        let now = Utc::now();
        let et =
            Expires::InDuration(std::time::Duration::from_secs(300)).to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            let diff = dt.signed_duration_since(now);
            assert!(diff.num_seconds() >= 299 && diff.num_seconds() <= 301);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    #[test]
    fn test_at_datetime_to_expiration_time() {
        let target = Utc::now() + Duration::hours(2);
        let et = Expires::AtDateTime(target).to_expiration_time(Utc::now(), 0);
        assert_eq!(et, ExpirationTime::At(target));
    }

    #[test]
    fn test_end_of_day_to_expiration_time() {
        let now = Utc::now();
        let et = Expires::EndOfDay { tz_offset: Some(0) }.to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            // Should be tomorrow 00:00 UTC
            assert!(dt > now);
            assert_eq!(dt.hour(), 0);
            assert_eq!(dt.minute(), 0);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    #[test]
    fn test_end_of_month_to_expiration_time() {
        let now = Utc::now();
        let et = Expires::EndOfMonth { tz_offset: Some(0) }.to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            assert!(dt > now);
            assert_eq!(dt.day(), 1); // 1st of next month
            assert_eq!(dt.hour(), 0);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    #[test]
    fn test_end_of_week_to_expiration_time() {
        let now = Utc::now();
        let et = Expires::EndOfWeek { tz_offset: Some(0) }.to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            assert!(dt > now);
            // Should be a Monday at 00:00
            assert_eq!(dt.weekday().num_days_from_monday(), 0);
            assert_eq!(dt.hour(), 0);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    #[test]
    fn test_on_day_of_week_to_expiration_time() {
        let now = Utc::now();
        // Target: Wednesday (day=2)
        let et = Expires::OnDayOfWeek {
            day: 2,
            tz_offset: Some(0),
        }
        .to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            assert!(dt > now);
            assert_eq!(dt.weekday().num_days_from_monday(), 2); // Wednesday
            assert_eq!(dt.hour(), 0);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    #[test]
    fn test_at_time_of_day_future_today() {
        // Use a time far enough in the future to be "today"
        let now = Utc::now();
        let target_hour = (now.hour() + 2) % 24;
        let et = Expires::AtTimeOfDay {
            hour: target_hour,
            minute: 0,
            second: 0,
            tz_offset: Some(0),
        }
        .to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            assert_eq!(dt.hour(), target_hour);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    // --- Parsing (FromStr) tests ---

    #[test]
    fn test_parse_never() {
        let e: Expires = "never".parse().unwrap();
        assert_eq!(e, Expires::Never);
        let e: Expires = "Never".parse().unwrap();
        assert_eq!(e, Expires::Never);
        let e: Expires = "NEVER".parse().unwrap();
        assert_eq!(e, Expires::Never);
    }

    #[test]
    fn test_parse_immediately() {
        let e: Expires = "immediately".parse().unwrap();
        assert_eq!(e, Expires::Immediately);
        let e: Expires = "Immediately".parse().unwrap();
        assert_eq!(e, Expires::Immediately);
    }

    #[test]
    fn test_parse_eod() {
        let e: Expires = "EOD".parse().unwrap();
        assert_eq!(e, Expires::EndOfDay { tz_offset: None });
        let e: Expires = "end of day".parse().unwrap();
        assert_eq!(e, Expires::EndOfDay { tz_offset: None });
        let e: Expires = "End Of Day".parse().unwrap();
        assert_eq!(e, Expires::EndOfDay { tz_offset: None });
    }

    #[test]
    fn test_parse_end_of_week() {
        let e: Expires = "end of week".parse().unwrap();
        assert_eq!(e, Expires::EndOfWeek { tz_offset: None });
        let e: Expires = "EOW".parse().unwrap();
        assert_eq!(e, Expires::EndOfWeek { tz_offset: None });
    }

    #[test]
    fn test_parse_end_of_month() {
        let e: Expires = "end of month".parse().unwrap();
        assert_eq!(e, Expires::EndOfMonth { tz_offset: None });
        let e: Expires = "EOM".parse().unwrap();
        assert_eq!(e, Expires::EndOfMonth { tz_offset: None });
    }

    #[test]
    fn test_parse_on_day_of_week() {
        let e: Expires = "on Monday".parse().unwrap();
        assert_eq!(e, Expires::OnDayOfWeek { day: 0, tz_offset: None });
        let e: Expires = "on tuesday".parse().unwrap();
        assert_eq!(e, Expires::OnDayOfWeek { day: 1, tz_offset: None });
        let e: Expires = "on Sunday".parse().unwrap();
        assert_eq!(e, Expires::OnDayOfWeek { day: 6, tz_offset: None });
    }

    #[test]
    fn test_parse_at_time_of_day() {
        let e: Expires = "at 12:00".parse().unwrap();
        assert_eq!(e, Expires::AtTimeOfDay { hour: 12, minute: 0, second: 0, tz_offset: None });
        let e: Expires = "at 08:30".parse().unwrap();
        assert_eq!(e, Expires::AtTimeOfDay { hour: 8, minute: 30, second: 0, tz_offset: None });
        let e: Expires = "at 23:59:59".parse().unwrap();
        assert_eq!(e, Expires::AtTimeOfDay { hour: 23, minute: 59, second: 59, tz_offset: None });
    }

    #[test]
    fn test_parse_at_time_with_timezone() {
        let e: Expires = "at 12:00 UTC".parse().unwrap();
        assert_eq!(e, Expires::AtTimeOfDay { hour: 12, minute: 0, second: 0, tz_offset: Some(0) });
        let e: Expires = "at 08:30 EST".parse().unwrap();
        assert_eq!(e, Expires::AtTimeOfDay { hour: 8, minute: 30, second: 0, tz_offset: Some(-18000) });
    }

    #[test]
    fn test_parse_duration_with_in() {
        let e: Expires = "in 5 min".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(300)));
        let e: Expires = "in 1 hour".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(3600)));
        let e: Expires = "in 30 seconds".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(30)));
    }

    #[test]
    fn test_parse_duration_without_in() {
        let e: Expires = "5 min".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(300)));
        let e: Expires = "1 hour".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(3600)));
    }

    #[test]
    fn test_parse_duration_compact() {
        let e: Expires = "5min".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(300)));
        let e: Expires = "30s".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(30)));
        let e: Expires = "2h".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(7200)));
        let e: Expires = "100ms".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_millis(100)));
    }

    #[test]
    fn test_parse_duration_various_units() {
        let e: Expires = "in 2 days".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(2 * 86400)));
        let e: Expires = "in 1 week".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(7 * 86400)));
        let e: Expires = "in 1 month".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(30 * 86400)));
    }

    #[test]
    fn test_parse_absolute_date() {
        let e: Expires = "2026-03-01".parse().unwrap();
        if let Expires::AtDateTime(dt) = e {
            assert_eq!(dt.year(), 2026);
            assert_eq!(dt.month(), 3);
            assert_eq!(dt.day(), 1);
        } else {
            panic!("Expected AtDateTime");
        }
    }

    #[test]
    fn test_parse_absolute_datetime() {
        let e: Expires = "2026-03-01 15:00".parse().unwrap();
        if let Expires::AtDateTime(dt) = e {
            assert_eq!(dt.year(), 2026);
            assert_eq!(dt.month(), 3);
            assert_eq!(dt.day(), 1);
            assert_eq!(dt.hour(), 15);
            assert_eq!(dt.minute(), 0);
        } else {
            panic!("Expected AtDateTime");
        }
    }

    #[test]
    fn test_parse_iso8601() {
        let e: Expires = "2026-03-01T15:00:00Z".parse().unwrap();
        if let Expires::AtDateTime(dt) = e {
            assert_eq!(dt.year(), 2026);
            assert_eq!(dt.hour(), 15);
        } else {
            panic!("Expected AtDateTime");
        }
    }

    #[test]
    fn test_parse_invalid() {
        let result: Result<Expires, _> = "gibberish".parse();
        assert!(result.is_err());
        let result: Result<Expires, _> = "".parse();
        assert!(result.is_err());
    }

    // --- Display tests ---

    #[test]
    fn test_display_never() {
        assert_eq!(Expires::Never.to_string(), "never");
    }

    #[test]
    fn test_display_immediately() {
        assert_eq!(Expires::Immediately.to_string(), "immediately");
    }

    #[test]
    fn test_display_duration() {
        assert_eq!(
            Expires::InDuration(std::time::Duration::from_secs(300)).to_string(),
            "in 5 min"
        );
        assert_eq!(
            Expires::InDuration(std::time::Duration::from_secs(3600)).to_string(),
            "in 1 hour"
        );
        assert_eq!(
            Expires::InDuration(std::time::Duration::from_secs(90)).to_string(),
            "in 90 seconds"
        );
    }

    #[test]
    fn test_display_end_of_day() {
        assert_eq!(
            Expires::EndOfDay { tz_offset: None }.to_string(),
            "end of day"
        );
    }

    #[test]
    fn test_display_end_of_week() {
        assert_eq!(
            Expires::EndOfWeek { tz_offset: None }.to_string(),
            "end of week"
        );
    }

    #[test]
    fn test_display_end_of_month() {
        assert_eq!(
            Expires::EndOfMonth { tz_offset: None }.to_string(),
            "end of month"
        );
    }

    #[test]
    fn test_display_on_day() {
        assert_eq!(
            Expires::OnDayOfWeek { day: 0, tz_offset: None }.to_string(),
            "on Monday"
        );
        assert_eq!(
            Expires::OnDayOfWeek { day: 4, tz_offset: None }.to_string(),
            "on Friday"
        );
    }

    #[test]
    fn test_display_at_time() {
        assert_eq!(
            Expires::AtTimeOfDay { hour: 12, minute: 0, second: 0, tz_offset: None }.to_string(),
            "at 12:00"
        );
        assert_eq!(
            Expires::AtTimeOfDay { hour: 8, minute: 30, second: 0, tz_offset: Some(0) }.to_string(),
            "at 08:30 UTC"
        );
        assert_eq!(
            Expires::AtTimeOfDay { hour: 23, minute: 59, second: 59, tz_offset: None }.to_string(),
            "at 23:59:59"
        );
    }

    #[test]
    fn test_display_at_datetime() {
        let dt = Utc.with_ymd_and_hms(2026, 3, 1, 15, 0, 0).unwrap();
        assert_eq!(
            Expires::AtDateTime(dt).to_string(),
            "2026-03-01T15:00:00+00:00"
        );
    }

    // --- Roundtrip (parse → display → parse) ---

    #[test]
    fn test_roundtrip_never() {
        let original = Expires::Never;
        let s = original.to_string();
        let parsed: Expires = s.parse().unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_immediately() {
        let original = Expires::Immediately;
        let s = original.to_string();
        let parsed: Expires = s.parse().unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_duration() {
        let original = Expires::InDuration(std::time::Duration::from_secs(3600));
        let s = original.to_string();
        let parsed: Expires = s.parse().unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_end_of_day() {
        let original = Expires::EndOfDay { tz_offset: None };
        let s = original.to_string();
        let parsed: Expires = s.parse().unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_on_day() {
        let original = Expires::OnDayOfWeek { day: 2, tz_offset: None }; // Wednesday
        let s = original.to_string();
        let parsed: Expires = s.parse().unwrap();
        assert_eq!(original, parsed);
    }

    // --- Serde tests ---

    #[test]
    fn test_serde_expires_json() {
        let original = Expires::InDuration(std::time::Duration::from_secs(300));
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"in 5 min\"");
        let parsed: Expires = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_serde_expires_never_json() {
        let original = Expires::Never;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"never\"");
        let parsed: Expires = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_serde_expiration_time_never() {
        let original = ExpirationTime::Never;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"never\"");
        let parsed: ExpirationTime = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_serde_expiration_time_immediately() {
        let original = ExpirationTime::Immediately;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"immediately\"");
        let parsed: ExpirationTime = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_serde_expiration_time_at() {
        let dt = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
        let original = ExpirationTime::At(dt);
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ExpirationTime = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }
}
