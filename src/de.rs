//! Forgiving deserializers for the API's loosely typed fields.
//!
//! Kavenegar is inconsistent about JSON types: the same counter arrives as
//! `120` on one endpoint and `"120"` on another, and optional text fields come
//! back as `null` rather than being left out. Strict serde types would turn
//! each of those into a decode failure that throws away an otherwise good
//! response, so the models route those fields through these helpers.

use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::Value;

/// Accepts an integer sent as a number, a numeric string, or `null`.
///
/// `null` and an empty string read as `0`. Anything else that is not an
/// integer — a fraction, a word — is a decode error rather than a silent `0`.
pub(crate) fn lenient_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(0),
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| de::Error::custom(format!("`{n}` is not a 64-bit integer"))),
        Value::String(s) if s.trim().is_empty() => Ok(0),
        Value::String(s) => s
            .trim()
            .parse()
            .map_err(|_| de::Error::custom(format!("`{s}` is not an integer"))),
        other => Err(de::Error::custom(format!(
            "expected an integer, got {other}"
        ))),
    }
}

/// Accepts a string, `null` (read as empty), or a bare number or boolean
/// (read as its JSON text).
#[cfg(any(
    feature = "sms",
    feature = "verify",
    feature = "call",
    feature = "account",
    feature = "line",
    feature = "media",
    feature = "utils"
))]
pub(crate) fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(lenient_opt_string(d)?.unwrap_or_default())
}

/// Like [`lenient_string`], but keeps `null` distinct from an empty string.
#[cfg(any(
    feature = "sms",
    feature = "verify",
    feature = "call",
    feature = "account",
    feature = "line",
    feature = "media",
    feature = "utils"
))]
pub(crate) fn lenient_opt_string<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s)),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::Bool(b) => Ok(Some(b.to_string())),
        other => Err(de::Error::custom(format!("expected a string, got {other}"))),
    }
}

/// Accepts `true`/`false`, `1`/`0`, their string forms, or `null` (false).
#[cfg(feature = "media")]
pub(crate) fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(false),
        Value::Bool(b) => Ok(b),
        Value::Number(n) => Ok(n.as_i64().is_some_and(|n| n != 0)),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" | "" => Ok(false),
            _ => Err(de::Error::custom(format!("`{s}` is not a boolean"))),
        },
        other => Err(de::Error::custom(format!(
            "expected a boolean, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Int(#[serde(deserialize_with = "lenient_i64")] i64);

    #[test]
    fn integers_arrive_as_numbers_strings_or_null() {
        let read = |json: &str| serde_json::from_str::<Int>(json).map(|v| v.0);
        assert_eq!(read("120").unwrap(), 120);
        assert_eq!(read(r#""120""#).unwrap(), 120);
        assert_eq!(read(r#"" 7 ""#).unwrap(), 7);
        assert_eq!(read("null").unwrap(), 0);
        assert_eq!(read(r#""""#).unwrap(), 0);
        assert!(read(r#""abc""#).is_err());
        assert!(read("1.5").is_err());
    }

    #[cfg(feature = "sms")]
    #[test]
    fn strings_tolerate_null_and_numbers() {
        #[derive(Deserialize)]
        struct S(#[serde(deserialize_with = "lenient_string")] String);
        let read = |json: &str| serde_json::from_str::<S>(json).map(|v| v.0).unwrap();
        assert_eq!(read(r#""x""#), "x");
        assert_eq!(read("null"), "");
        assert_eq!(read("450"), "450");
    }
}
