//! Normalizing the reset timestamps returned by provider usage endpoints.
//!
//! Providers are inconsistent about how they express "when does this window
//! reset": the same field has been seen as an RFC 3339 string, as a Unix epoch
//! in seconds, and as a Unix epoch in milliseconds — sometimes as a JSON number
//! and sometimes as a numeric string. Everything downstream works in Unix
//! milliseconds, so the ambiguity is resolved here and nowhere else.
//!
//! This module is pure: no I/O, no clock reads, no network.

use chrono::{DateTime, NaiveDateTime};
use serde::Deserialize;

/// The largest plausible seconds epoch and the smallest plausible milliseconds
/// epoch are far apart, so a single threshold separates them without needing
/// any extra metadata from the provider.
///
/// `10_000_000_000` as seconds is 2286-11-20; as milliseconds it is 1970-04-26.
/// Any real reset timestamp is decades away from both, so a value above the
/// threshold is milliseconds and a value at or below it is seconds.
const SECONDS_MILLISECONDS_THRESHOLD: f64 = 10_000_000_000.0;

/// A reset value exactly as it arrives in JSON: either a number or a string.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ResetValue {
    Number(f64),
    Text(String),
}

/// Convert an optional reset value into Unix milliseconds.
///
/// Returns `None` for anything that cannot be understood. A missing reset time
/// is a normal condition — the UI simply omits it — so an unparseable value must
/// never be an error, and must never be guessed at.
pub fn parse_reset_timestamp(value: Option<&ResetValue>) -> Option<i64> {
    match value? {
        ResetValue::Number(number) => from_epoch_number(*number),
        ResetValue::Text(text) => from_text(text),
    }
}

/// Interpret a numeric epoch, choosing seconds or milliseconds by magnitude.
pub fn from_epoch_number(number: f64) -> Option<i64> {
    if !number.is_finite() {
        return None;
    }

    // Why: a non-positive epoch is not a real reset time, it is a zeroed or
    // sentinel field. Scaling it would produce a confident-looking 1970 date.
    if number <= 0.0 {
        return None;
    }

    let millis = if number > SECONDS_MILLISECONDS_THRESHOLD {
        number
    } else {
        number * 1000.0
    };

    if millis > i64::MAX as f64 {
        return None;
    }

    Some(millis as i64)
}

/// Interpret a string reset value.
///
/// A numeric string is treated as an epoch, matching how these fields have been
/// observed to arrive; anything else is parsed as a date.
pub fn from_text(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(number) = trimmed.parse::<f64>() {
        return from_epoch_number(number);
    }

    from_datetime_text(trimmed)
}

/// Parse a date string. RFC 3339 is the documented shape; the naive fallbacks
/// exist because a timezone-less timestamp would otherwise silently become "no
/// reset time" in the UI.
fn from_datetime_text(text: &str) -> Option<i64> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp_millis());
    }

    // Assume UTC for a timestamp that omits its offset. Being an hour or two
    // wrong on a countdown is a better failure than showing nothing at all.
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
            return Some(naive.and_utc().timestamp_millis());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-09-05T04:00:00Z
    const SECONDS: f64 = 1_788_580_800.0;
    const MILLIS: i64 = 1_788_580_800_000;

    #[test]
    fn parses_a_seconds_epoch_number() {
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Number(SECONDS))),
            Some(MILLIS)
        );
    }

    #[test]
    fn parses_a_milliseconds_epoch_number() {
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Number(MILLIS as f64))),
            Some(MILLIS)
        );
    }

    #[test]
    fn parses_an_rfc3339_string() {
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text("2026-09-05T04:00:00Z".into()))),
            Some(MILLIS)
        );
    }

    #[test]
    fn parses_an_rfc3339_string_with_an_offset() {
        // Same instant, expressed in +02:00.
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text("2026-09-05T06:00:00+02:00".into()))),
            Some(MILLIS)
        );
    }

    #[test]
    fn parses_a_numeric_string_as_an_epoch() {
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text(format!("{}", SECONDS as i64)))),
            Some(MILLIS)
        );
    }

    #[test]
    fn assumes_utc_for_a_timestamp_without_an_offset() {
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text("2026-09-05T04:00:00".into()))),
            Some(MILLIS)
        );
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text("2026-09-05 04:00:00".into()))),
            Some(MILLIS)
        );
    }

    #[test]
    fn the_threshold_separates_seconds_from_milliseconds() {
        // At the threshold: still seconds.
        assert_eq!(
            from_epoch_number(SECONDS_MILLISECONDS_THRESHOLD),
            Some(10_000_000_000_000)
        );
        // One above: already milliseconds.
        assert_eq!(
            from_epoch_number(SECONDS_MILLISECONDS_THRESHOLD + 1.0),
            Some(10_000_000_001)
        );
    }

    // --- Everything that must yield None ---

    #[test]
    fn returns_none_for_an_absent_value() {
        assert_eq!(parse_reset_timestamp(None), None);
    }

    #[test]
    fn returns_none_for_an_empty_or_blank_string() {
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text(String::new()))),
            None
        );
        assert_eq!(
            parse_reset_timestamp(Some(&ResetValue::Text("   ".into()))),
            None
        );
    }

    #[test]
    fn returns_none_for_non_numeric_garbage() {
        for garbage in ["not a date", "tomorrow", "2026-13-45T99:99:99Z", "NaN-ish"] {
            assert_eq!(
                parse_reset_timestamp(Some(&ResetValue::Text(garbage.into()))),
                None,
                "expected None for {garbage:?}"
            );
        }
    }

    #[test]
    fn returns_none_for_non_positive_and_non_finite_numbers() {
        // Why: these are zeroed or sentinel fields, not reset times. Scaling
        // them would produce a confident-looking 1970 date in the UI.
        assert_eq!(from_epoch_number(0.0), None);
        assert_eq!(from_epoch_number(-1.0), None);
        assert_eq!(from_epoch_number(f64::NAN), None);
        assert_eq!(from_epoch_number(f64::INFINITY), None);
        assert_eq!(from_epoch_number(f64::NEG_INFINITY), None);
    }

    #[test]
    fn deserializes_from_either_json_shape() {
        // Guards the untagged enum: the same field arrives as a number from one
        // provider and as a string from another.
        let as_number: ResetValue = serde_json::from_str("1788580800").expect("number");
        let as_text: ResetValue = serde_json::from_str(r#""2026-09-05T04:00:00Z""#).expect("text");

        assert_eq!(parse_reset_timestamp(Some(&as_number)), Some(MILLIS));
        assert_eq!(parse_reset_timestamp(Some(&as_text)), Some(MILLIS));
    }
}
