//! Restricted data extraction from console server-rendered billing objects.
//! This is not a JavaScript evaluator. Strings, comments and nested values are
//! skipped, only allowlisted literal fields are decoded, and ambiguity fails.

use std::collections::BTreeMap;
use crate::providers::error::UsageError;
use crate::providers::usage::BillingUsage;

struct Frame { delimiter: u8, start: usize, fields: Vec<(usize, usize)> }

pub fn parse_billing(text: &str) -> Result<BillingUsage, UsageError> {
    if text.len() > 2_000_000 { return Err(UsageError::Parse); }
    let bytes = text.as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut found: Option<BillingUsage> = None;
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index];
        if matches!(ch, b'"' | b'\'' | b'`') {
            let quote = ch;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' { index += 2; continue; }
                if bytes[index] == quote { break; }
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' { index += 1; }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && &bytes[index..index + 2] != b"*/" { index += 1; }
            index += 1;
        } else if matches!(ch, b'{' | b'[' | b'(') {
            if stack.len() >= 128 { return Err(UsageError::Parse); }
            stack.push(Frame { delimiter: ch, start: index + 1, fields: Vec::new() });
        } else if ch == b',' {
            if let Some(frame) = stack.last_mut().filter(|f| f.delimiter == b'{') {
                if frame.fields.len() >= 256 { return Err(UsageError::Parse); }
                frame.fields.push((frame.start, index));
                frame.start = index + 1;
            }
        } else if matches!(ch, b'}' | b']' | b')') {
            if let Some(mut frame) = stack.pop() {
                if frame.delimiter == b'{' && ch == b'}' {
                    frame.fields.push((frame.start, index));
                    let pairs: Vec<_> = frame.fields.iter().filter_map(|(start,end)| {
                        let (key,value) = text.get(*start..*end)?.split_once(':')?;
                        let key = key.trim().trim_matches(['"', '\'']);
                        Some((key, value.trim()))
                    }).collect();
                    let fields: BTreeMap<_, _> = pairs.iter().copied().collect();
                    if fields.contains_key("balance") && fields.contains_key("monthlyUsage")
                        && fields.contains_key("monthlyLimit") && fields.contains_key("reload") {
                        if fields.len() != pairs.len() { return Err(UsageError::Parse); }
                        let billing = decode(&fields)?;
                        if found.as_ref().is_some_and(|prior| prior != &billing) { return Err(UsageError::Parse); }
                        found = Some(billing);
                    }
                }
            }
        }
        index += 1;
    }
    found.ok_or(UsageError::Parse)
}

fn optional_number(raw: &str) -> Result<Option<f64>, UsageError> {
    if matches!(raw, "null" | "undefined" | "void 0") { return Ok(None); }
    let value: f64 = raw.parse().map_err(|_| UsageError::Parse)?;
    if !value.is_finite() || value.abs() > 9_007_199_254_740_991.0 { return Err(UsageError::Parse); }
    Ok(Some(value))
}

fn date_literal(raw: &str) -> Option<i64> {
    let value = raw.strip_prefix("new Date(").and_then(|v| v.strip_suffix(')')).unwrap_or(raw);
    let value = value.trim().trim_matches(['"', '\'']);
    chrono::DateTime::parse_from_rfc3339(value).ok().map(|v| v.timestamp_millis())
        .or_else(|| value.parse::<i64>().ok().filter(|v| *v > 0))
}

fn decode(fields: &BTreeMap<&str, &str>) -> Result<BillingUsage, UsageError> {
    let balance = optional_number(fields["balance"])?.ok_or(UsageError::Parse)?;
    let spent = optional_number(fields["monthlyUsage"])?;
    let limit = optional_number(fields["monthlyLimit"])?;
    if spent.is_some_and(|v| v < 0.0) || limit.is_some_and(|v| v < 0.0) { return Err(UsageError::Parse); }
    Ok(BillingUsage {
        balance_usd: balance / 100_000_000.0,
        monthly_spend_usd: spent.map(|v| v / 100_000_000.0),
        monthly_limit_usd: limit,
        spend_updated_at: fields.get("timeMonthlyUsageUpdated").and_then(|v| date_literal(v)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn billing_units_and_dates_are_not_quota_percentages() {
        let billing = parse_billing(r#"<script>data=$R[1]={balance:1250000000,monthlyUsage:325000000,monthlyLimit:20,reload:true,timeMonthlyUsageUpdated:new Date("2026-09-08T12:00:00Z")}</script>"#).unwrap();
        assert_eq!(billing.balance_usd, 12.5);
        assert_eq!(billing.monthly_spend_usd, Some(3.25));
        assert_eq!(billing.monthly_limit_usd, Some(20.0));
        assert!(billing.spend_updated_at.is_some());
    }
    #[test]
    fn null_zero_and_negative_balance_stay_distinct() {
        let billing = parse_billing(r#"{"balance":-50000000,"monthlyUsage":0,"monthlyLimit":null,"reload":false}"#).unwrap();
        assert_eq!(billing.balance_usd, -0.5);
        assert_eq!(billing.monthly_spend_usd, Some(0.0));
        assert_eq!(billing.monthly_limit_usd, None);
        let billing = parse_billing("{balance:0,monthlyUsage:null,monthlyLimit:0,reload:false}").unwrap();
        assert_eq!(billing.monthly_spend_usd, None);
        assert_eq!(billing.monthly_limit_usd, Some(0.0));
    }
    #[test]
    fn ignores_strings_comments_nested_fields_and_ambiguous_billing() {
        for text in [
            "<html>Sign in</html>",
            "'{balance:1,monthlyUsage:2,monthlyLimit:3,reload:true}'",
            "/* {balance:1,monthlyUsage:2,monthlyLimit:3,reload:true} */",
            "{balance:1,monthlyUsage:{value:2},monthlyLimit:3,reload:true}",
            "{balance:1,monthlyUsage:2,monthlyLimit:3,reload:true};{balance:2,monthlyUsage:2,monthlyLimit:3,reload:true}",
            "{balance:NaN,monthlyUsage:2,monthlyLimit:3,reload:true}",
            "{balance:1,monthlyUsage:2,monthlyLimit:-3,reload:true}",
            "{balance:1,balance:2,monthlyUsage:2,monthlyLimit:3,reload:true}",
        ] { assert!(parse_billing(text).is_err(), "accepted an invalid billing object"); }
        assert!(parse_billing(&" ".repeat(2_000_001)).is_err());
        assert!(parse_billing(&"{".repeat(129)).is_err());
    }
}
