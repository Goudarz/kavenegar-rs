//! Local checks on values that end up in a request.
//!
//! These exist for safety as much as convenience. Kavenegar takes several
//! lists as comma-separated strings, so a recipient value such as
//! `"0912…,0913…"` silently becomes two recipients. When that value comes
//! from an end user — the phone number typed into a sign-in form — that is an
//! SMS-pumping hole: one request makes your account pay for up to 200 sends.
//! Rejecting anything that is not a plain number closes it.

use crate::error::{Error, Result};

/// The longest phone or line number accepted, in digits.
///
/// E.164 allows 15 digits; the extra room covers the `00` international
/// prefix Kavenegar documents for lookups.
const MAX_NUMBER_DIGITS: usize = 20;

/// Accepts a phone or line number in any format Kavenegar documents:
/// `09121234567`, `9121234567`, `00989121234567` or `+989121234567`.
///
/// Anything else — separators, spaces, Persian digits, letters — is rejected,
/// so one value can never stand for more than one number.
#[cfg_attr(
    not(any(
        feature = "sms",
        feature = "verify",
        feature = "call",
        feature = "line"
    )),
    allow(dead_code)
)]
pub(crate) fn number(parameter: &'static str, value: &str) -> Result<()> {
    let digits = value.strip_prefix('+').unwrap_or(value);
    if digits.is_empty() {
        return Err(Error::invalid(parameter, "must not be empty"));
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::invalid(
            parameter,
            format!(
                "`{value}` is not a phone or line number; use ASCII digits with an \
                 optional leading `+`, one number per value"
            ),
        ));
    }
    if digits.len() > MAX_NUMBER_DIGITS {
        return Err(Error::invalid(
            parameter,
            format!("`{value}` is longer than {MAX_NUMBER_DIGITS} digits"),
        ));
    }
    Ok(())
}

/// Checks every number in a list. See [`number`].
#[cfg_attr(
    not(any(feature = "sms", feature = "call", feature = "line")),
    allow(dead_code)
)]
pub(crate) fn numbers(parameter: &'static str, values: &[String]) -> Result<()> {
    values.iter().try_for_each(|v| number(parameter, v))
}

/// The longest tag Kavenegar accepts, in characters.
const MAX_TAG_CHARS: usize = 200;

/// Applies Kavenegar's tag rules, which it otherwise answers with a 607:
/// at most 200 characters of English letters and digits, plus `-` and `_`.
///
/// Kavenegar's docs write the second allowed separator as `ـ` (the Persian
/// kashida, which a Persian keyboard produces for Shift+`-`), so that is
/// accepted too.
#[cfg_attr(
    not(any(feature = "sms", feature = "verify", feature = "call")),
    allow(dead_code)
)]
pub(crate) fn tag(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::invalid("tag", "must not be empty"));
    }
    if value.chars().count() > MAX_TAG_CHARS {
        return Err(Error::invalid(
            "tag",
            format!("must be at most {MAX_TAG_CHARS} characters"),
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | 'ـ'))
    {
        return Err(Error::invalid(
            "tag",
            "may contain only English letters, digits, `-` and `_`",
        ));
    }
    Ok(())
}

/// Rejects an API key that would change the shape of the request URL.
///
/// The key is placed in the URL path as-is. Whitespace — a trailing newline
/// from a secrets file is the usual culprit — or a `/`, `?`, `#` or `%` would
/// silently point the request somewhere else, so only characters that are
/// literal in a URL path segment are allowed.
pub(crate) fn api_key(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::invalid("api_key", "must not be empty"));
    }
    let safe = value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'='));
    // `.` and `..` are legal characters but dot-segments the URL parser
    // would resolve away.
    if !safe || value.bytes().all(|b| b == b'.') {
        // The key itself is deliberately left out of the message.
        return Err(Error::invalid(
            "api_key",
            "contains whitespace or a character that is not allowed in a URL path; \
             check for a stray newline or quote around the key",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_number_formats_are_accepted() {
        for ok in [
            "09121234567",
            "9121234567",
            "00989121234567",
            "+989121234567",
            "10004346",
        ] {
            assert!(number("receptor", ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn a_value_that_could_smuggle_extra_recipients_is_refused() {
        for bad in [
            "09121234567,09131234567",
            "0912 123 4567",
            "09121234567\n",
            "۰۹۱۲۱۲۳۴۵۶۷",
            "+",
            "",
            "abc",
            "++98912",
            "123456789012345678901",
        ] {
            assert!(number("receptor", bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tags_follow_the_documented_alphabet() {
        assert!(tag("signup-2026_v1").is_ok());
        assert!(tag(&"a".repeat(200)).is_ok());
        assert!(tag(&"a".repeat(201)).is_err());
        assert!(tag("has space").is_err());
        assert!(tag("a&b").is_err());
        assert!(tag("").is_err());
    }

    #[test]
    fn api_keys_that_would_bend_the_url_are_refused() {
        assert!(api_key("613472435563797A37677331D").is_ok());
        assert!(api_key("abc=").is_ok());
        for bad in ["key\n", " key", "a/b", "a?b", "a#b", "a%2Fb", "../x", ".."] {
            assert!(api_key(bad).is_err(), "{bad:?}");
        }
    }
}
