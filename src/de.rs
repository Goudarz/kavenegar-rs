//! Forgiving deserializers for the API's loosely typed fields.
//!
//! Kavenegar is inconsistent about JSON types: the same counter arrives as
//! `120` on one endpoint and `"120"` on another, and optional text fields come
//! back as `null` rather than being left out. Strict serde types would turn
//! each of those into a decode failure that throws away an otherwise good
//! response, so the models route those fields through these helpers.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};

/// Accepts an integer sent as a number, a numeric string, or `null`.
///
/// `null` and an empty string read as `0`. Anything else that is not an
/// integer — a fraction, a word — is a decode error rather than a silent `0`.
pub(crate) fn lenient_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    struct V;

    impl Visitor<'_> for V {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an integer, a numeric string, or null")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(|_| E::custom("integer does not fit in 64 bits"))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            let v = v.trim();
            if v.is_empty() {
                return Ok(0);
            }
            v.parse()
                .map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))
        }

        fn visit_unit<E: de::Error>(self) -> Result<i64, E> {
            Ok(0)
        }

        fn visit_none<E: de::Error>(self) -> Result<i64, E> {
            Ok(0)
        }
    }

    d.deserialize_any(V)
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
    struct V;

    impl Visitor<'_> for V {
        type Value = Option<String>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string, a number, a boolean, or null")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(v.to_owned()))
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            Ok(Some(v))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    d.deserialize_any(V)
}

/// Accepts `true`/`false`, `1`/`0`, their string forms, or `null` (false).
#[cfg(feature = "media")]
pub(crate) fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    struct V;

    impl Visitor<'_> for V {
        type Value = bool;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a boolean, 0 or 1, their string forms, or null")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<bool, E> {
            let v = v.trim();
            if v.eq_ignore_ascii_case("true") || v == "1" {
                Ok(true)
            } else if v.eq_ignore_ascii_case("false") || v == "0" || v.is_empty() {
                Ok(false)
            } else {
                Err(E::invalid_value(de::Unexpected::Str(v), &self))
            }
        }

        fn visit_unit<E: de::Error>(self) -> Result<bool, E> {
            Ok(false)
        }

        fn visit_none<E: de::Error>(self) -> Result<bool, E> {
            Ok(false)
        }
    }

    d.deserialize_any(V)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

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
