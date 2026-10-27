//! Error types returned by this crate.

use std::fmt;

/// Convenient alias for fallible operations in this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Anything that can go wrong while talking to the Kavenegar API.
///
/// # Logging
///
/// `Display` is safe to log: it never contains the API key, and never contains
/// a response body. `Debug` also omits the API key, but does include response
/// bodies, which may carry recipient numbers or message text.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The API answered, but its `return` envelope carried a failure status.
    #[error(transparent)]
    Api(#[from] ApiError),

    /// The request never produced a usable HTTP response.
    ///
    /// The underlying `reqwest` error is deliberately not exposed: Kavenegar
    /// puts the API key in the URL path, and `reqwest` prints the URL in its
    /// error messages, so passing it through would write the key into any log
    /// that formats this error.
    #[error("http request failed: {0}")]
    Http(TransportError),

    /// The body arrived but could not be decoded into the expected shape.
    ///
    /// The raw `body` is retained so callers can diagnose an API change.
    ///
    /// # Personal data
    ///
    /// A Kavenegar response body routinely contains recipient phone numbers,
    /// and may contain message text — including a one-time password. The body
    /// is kept out of `Display`, so `error!("{e}")` is safe to log, but it is
    /// present in `Debug`: treat `error!("{e:?}")` as a log line carrying
    /// personal data.
    ///
    /// The same applies to `source`: a serde message can quote the value it
    /// rejected, so `Display` reports only the kind of failure and where it
    /// happened, and the full message is reached through `source()` or
    /// `Debug`.
    #[error(
        "could not decode the response ({} error at line {}, column {})",
        decode_kind(source),
        source.line(),
        source.column()
    )]
    Decode {
        /// The underlying serde failure.
        #[source]
        source: serde_json::Error,
        /// The raw response body.
        body: String,
    },

    /// The server answered with an error status and a body that was not the
    /// usual JSON envelope — typically a proxy or gateway page.
    ///
    /// As with [`Error::Decode`], `body` may carry personal data and is shown
    /// by `Debug` but not by `Display`.
    #[error("server returned HTTP {status} with an unrecognised body")]
    UnexpectedStatus {
        /// The HTTP status code.
        status: u16,
        /// The raw response body.
        body: String,
    },

    /// The call succeeded but carried no `entries`, where one was required.
    #[error("`{method}` reported success but returned no entries")]
    MissingEntries {
        /// The endpoint that was called.
        method: String,
        /// The raw response body.
        body: String,
    },

    /// The response body exceeded the size this client will buffer.
    #[error("response body exceeded the {limit} byte limit")]
    ResponseTooLarge {
        /// The ceiling that was hit, in bytes.
        limit: usize,
    },

    /// A parameter was rejected locally, before any request was sent.
    #[error("invalid parameter `{parameter}`: {reason}")]
    InvalidParameter {
        /// Name of the offending parameter.
        parameter: &'static str,
        /// Why it was rejected.
        reason: String,
    },
}

/// Names a serde failure without quoting any of the data it was looking at.
fn decode_kind(source: &serde_json::Error) -> &'static str {
    match source.classify() {
        serde_json::error::Category::Io => "I/O",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "type",
        serde_json::error::Category::Eof => "unexpected end of input",
    }
}

impl Error {
    /// Wraps a transport failure, scrubbing the API key from its message.
    pub(crate) fn transport(source: &reqwest::Error, api_key: &str) -> Self {
        Error::Http(TransportError::new(source, api_key))
    }

    pub(crate) fn invalid(parameter: &'static str, reason: impl Into<String>) -> Self {
        Error::InvalidParameter {
            parameter,
            reason: reason.into(),
        }
    }

    /// The API status code, when this error came from the API envelope.
    #[must_use]
    pub fn api_status(&self) -> Option<ApiStatus> {
        match self {
            Error::Api(e) => Some(e.status()),
            _ => None,
        }
    }

    /// Whether retrying the identical request could plausibly succeed.
    ///
    /// Covers transport hiccups and the API's own "busy" and "rate limited"
    /// statuses. Never true for a bad API key, malformed recipients, or any
    /// other problem a retry would reproduce exactly.
    ///
    /// # Retrying a send can send twice
    ///
    /// A timeout means no answer arrived, not that nothing happened:
    /// Kavenegar may already have accepted the message. Retrying `send`,
    /// `send_array`, `lookup` or `text_to_speech` after a timeout can deliver
    /// it twice and charge for both. For `send`, `send_array` and
    /// `text_to_speech`, attach `local_ids` — Kavenegar does not send a
    /// repeated local id again — before retrying; for a one-time password,
    /// prefer issuing a fresh code over resending the old one.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Api(e) => e.status().is_retryable(),
            Error::Http(e) => e.is_timeout() || e.is_connect(),
            Error::UnexpectedStatus { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

/// A transport-level failure, with the request URL stripped out.
///
/// Kavenegar authenticates by putting the API key in the URL path, so any
/// error carrying that URL carries the credential with it. This type keeps the
/// diagnosis — whether the failure was a timeout, a refused connection, a TLS
/// problem — while guaranteeing the key never reaches a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    kind: TransportKind,
    detail: String,
}

/// What went wrong at the transport level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransportKind {
    /// The connection could not be established.
    Connect,
    /// The request or response exceeded the timeout.
    Timeout,
    /// The connection failed partway through the body.
    Body,
    /// Too many redirects, or a redirect that could not be followed.
    Redirect,
    /// Anything else.
    Other,
}

impl TransportError {
    fn new(source: &reqwest::Error, api_key: &str) -> Self {
        let kind = if source.is_timeout() {
            TransportKind::Timeout
        } else if source.is_connect() {
            TransportKind::Connect
        } else if source.is_body() {
            TransportKind::Body
        } else if source.is_redirect() {
            TransportKind::Redirect
        } else {
            TransportKind::Other
        };

        // Take the innermost cause: it describes the real failure without
        // repeating the URL that the outer layers embed.
        let mut deepest: &dyn std::error::Error = source;
        while let Some(next) = deepest.source() {
            deepest = next;
        }

        Self {
            kind,
            detail: redact(&deepest.to_string(), api_key),
        }
    }

    /// What kind of transport failure this was.
    #[must_use]
    pub fn kind(&self) -> TransportKind {
        self.kind
    }

    /// The underlying cause, with the API key removed.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Whether the request timed out.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.kind == TransportKind::Timeout
    }

    /// Whether the connection could not be established.
    #[must_use]
    pub fn is_connect(&self) -> bool {
        self.kind == TransportKind::Connect
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            TransportKind::Connect => "could not connect",
            TransportKind::Timeout => "timed out",
            TransportKind::Body => "the connection dropped mid-body",
            TransportKind::Redirect => "the redirect could not be followed",
            TransportKind::Other => "the request failed",
        };
        write!(f, "{what}: {}", self.detail)
    }
}

impl std::error::Error for TransportError {}

/// Removes the API key from a string that is about to be shown to a human.
///
/// Short keys are dropped rather than matched, so a one- or two-character key
/// cannot turn every stray character in the message into a redaction.
pub(crate) fn redact(text: &str, api_key: &str) -> String {
    if api_key.len() < 8 {
        return text.to_owned();
    }
    text.replace(api_key, "[REDACTED]")
}

/// Makes server-supplied text safe to put in a log line.
///
/// Control characters (newlines that would forge extra log entries, ANSI
/// escapes that would rewrite a terminal) and Unicode direction overrides
/// (which can make text display differently from what it says) become
/// spaces, and the result is capped at `max_chars`.
pub(crate) fn printable(text: &str, max_chars: usize) -> String {
    let mut out: String = text
        .chars()
        .take(max_chars)
        .map(|c| {
            let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
            if c.is_control() || bidi { ' ' } else { c }
        })
        .collect();
    if text.chars().nth(max_chars).is_some() {
        out.push('…');
    }
    out
}

/// A failure reported by Kavenegar inside the `return` envelope.
///
/// Kavenegar answers with HTTP 200 more often than not, carrying the real
/// outcome in the body, so this is the error you will match on most.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    status: ApiStatus,
    message: String,
}

impl ApiError {
    /// Builds an error from a status code and the API's own message.
    pub fn new(status: impl Into<ApiStatus>, message: impl Into<String>) -> Self {
        Self {
            status: status.into(),
            message: message.into(),
        }
    }

    /// The status Kavenegar reported.
    #[must_use]
    pub fn status(&self) -> ApiStatus {
        self.status
    }

    /// The numeric status code, including codes this crate does not name.
    #[must_use]
    pub fn code(&self) -> i32 {
        self.status.code()
    }

    /// The message Kavenegar sent, verbatim (usually Persian).
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "kavenegar error {}: {}",
            self.status.code(),
            self.status.describe()
        )?;
        if !self.message.is_empty() {
            write!(f, " ({})", self.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for ApiError {}

macro_rules! api_status {
    ($( $variant:ident = $code:literal, $desc:literal; )*) => {
        /// Status codes carried in the `return.status` field.
        ///
        /// Codes this crate does not name are preserved in [`ApiStatus::Other`],
        /// so a newly introduced server-side code never becomes a decode failure.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum ApiStatus {
            $(
                #[doc = $desc]
                $variant,
            )*
            /// A code this crate does not recognise.
            Other(i32),
        }

        impl ApiStatus {
            /// The numeric code as sent by the API.
            pub fn code(self) -> i32 {
                match self {
                    $( ApiStatus::$variant => $code, )*
                    ApiStatus::Other(c) => c,
                }
            }

            /// A short English explanation of the code.
            pub fn describe(self) -> &'static str {
                match self {
                    $( ApiStatus::$variant => $desc, )*
                    ApiStatus::Other(_) => "unrecognised status code",
                }
            }
        }

        impl From<i32> for ApiStatus {
            fn from(code: i32) -> Self {
                match code {
                    $( $code => ApiStatus::$variant, )*
                    other => ApiStatus::Other(other),
                }
            }
        }
    };
}

api_status! {
    Ok = 200, "request completed successfully";
    MissingParameters = 400, "the request is missing required parameters";
    AccountInactive = 401, "the account is not active";
    OperationFailed = 402, "the operation could not be completed";
    InvalidApiKey = 403, "the API key is invalid or revoked";
    MethodNotFound = 404, "no such API method";
    MethodNotAllowed = 405, "the method was called with the wrong HTTP verb";
    EmptyParameters = 406, "one or more required parameters were empty";
    AccessDenied = 407, "access denied; the method requires an IP allowlist entry";
    ServerBusy = 409, "the Kavenegar server is temporarily unavailable";
    InvalidReceptor = 411, "the recipient number is invalid";
    InvalidSender = 412, "the sender line is invalid";
    InvalidMessage = 413, "the message is empty or longer than the allowed limit";
    BatchTooLarge = 414, "the batch exceeds the per-call limit";
    IndexOutOfRange = 415, "the start index is beyond the total record count";
    IpMismatch = 416, "the calling IP does not match the allowlist";
    InvalidDate = 417, "the date is malformed or outside the accepted window";
    InsufficientCredit = 418, "the account has insufficient credit";
    ArrayLengthMismatch = 419, "the supplied arrays have differing lengths";
    LinksNotPermitted = 420, "this account may not put links in a message body";
    InvalidCharacters = 422, "the data contains characters that cannot be processed";
    TemplateNotFound = 424, "no approved verification template has that name";
    ServicePlanRequired = 426, "this method requires the advanced service plan";
    LineAccessRequired = 427, "using this line requires an access level to be granted";
    TokenNotDialable = 428, "a voice call cannot read out a non-numeric token";
    IpRateLimited = 429, "too many requests from this IP";
    AccountNotVerified = 430, "the account has not completed identity verification";
    InvalidCodeStructure = 431, "a token contains a newline, space, underscore or separator";
    TokenNotInTemplate = 432, "the token parameter does not appear in the template";
    IpThrottled = 451, "the IP is throttled for exceeding the request limit";
    TestSendOnly = 501, "a trial account may only send to its own owner";
    TooManyNumbers = 604, "too many numbers in one call; the limit is 200";
    InvalidTag = 607, "the supplied tag name is invalid";
    MediaRequired = 800, "a media file is required for this request";
    MediaInvalid = 801, "the media file is invalid or cannot be processed";
    MediaTooLarge = 802, "the media file exceeds the size limit";
    MediaBadAspectRatio = 803, "the media file's aspect ratio is not allowed";
    MediaTooLong = 804, "the media file's duration exceeds the limit";
    MediaUploadDenied = 805, "this account may not upload media";
    MediaQuotaExceeded = 806, "the media upload quota has been used up";
    MediaDuplicate = 807, "this media file has already been uploaded";
    MediaIdInvalid = 808, "the media identifier is invalid";
    MediaNotFound = 809, "no media file with that identifier exists";
    MediaPendingReview = 810, "the media file is still awaiting moderation";
    MediaRejected = 811, "the media file was rejected by moderation";
    MediaNotAllowedByPolicy = 812, "the selected policy does not permit media";
    MediaLineUnsupported = 813, "only domestic messenger lines can send media";
}

impl ApiStatus {
    /// Whether this code represents success.
    #[must_use]
    pub fn is_success(self) -> bool {
        self == ApiStatus::Ok
    }

    /// Whether retrying the identical request could plausibly succeed.
    #[must_use]
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            ApiStatus::ServerBusy
                | ApiStatus::IpRateLimited
                | ApiStatus::IpThrottled
                | ApiStatus::OperationFailed
        )
    }
}

impl fmt::Display for ApiStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.code(), self.describe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_round_trip() {
        assert_eq!(ApiStatus::from(200), ApiStatus::Ok);
        assert_eq!(ApiStatus::from(418), ApiStatus::InsufficientCredit);
        assert_eq!(ApiStatus::InvalidApiKey.code(), 403);
    }

    #[test]
    fn the_media_and_account_codes_are_named() {
        // 430 is not in Kavenegar's published table; it turned up only when
        // calling the live API with an unverified account.
        assert_eq!(ApiStatus::from(430), ApiStatus::AccountNotVerified);
        assert_eq!(ApiStatus::from(420), ApiStatus::LinksNotPermitted);
        assert_eq!(ApiStatus::from(427), ApiStatus::LineAccessRequired);
        assert_eq!(ApiStatus::from(604), ApiStatus::TooManyNumbers);

        for code in 800..=813 {
            assert!(
                !matches!(ApiStatus::from(code), ApiStatus::Other(_)),
                "media code {code} should be named"
            );
        }

        // None of them are worth retrying unchanged.
        for code in [420, 427, 430, 604, 800, 809, 813] {
            assert!(!ApiStatus::from(code).is_retryable(), "{code}");
        }
    }

    #[test]
    fn server_text_cannot_forge_log_lines_or_flip_direction() {
        let hostile = "ok\n2026-01-01 ERROR forged\u{1b}[31m\u{202E}evil";
        let clean = printable(hostile, 512);
        assert!(!clean.chars().any(|c| c.is_control()), "{clean:?}");
        assert!(!clean.contains('\u{202E}'));
        assert!(clean.starts_with("ok "));

        let long = "ب".repeat(600);
        let capped = printable(&long, 512);
        assert_eq!(capped.chars().count(), 513);
        assert!(capped.ends_with('…'));
    }

    #[test]
    fn decode_display_does_not_quote_the_rejected_value() {
        #[derive(Debug, serde::Deserialize)]
        struct T {
            #[allow(dead_code)]
            n: i64,
        }
        let source = serde_json::from_str::<T>(r#"{"n":"09121234567 secret-otp"}"#).unwrap_err();
        // serde itself does quote it...
        assert!(source.to_string().contains("secret-otp"));
        let err = Error::Decode {
            source,
            body: String::new(),
        };
        // ...but this crate's Display does not.
        let shown = err.to_string();
        assert!(
            !shown.contains("secret-otp") && !shown.contains("0912"),
            "{shown}"
        );
        assert!(shown.contains("type error"), "{shown}");
    }

    #[test]
    fn unknown_codes_are_preserved() {
        let s = ApiStatus::from(9999);
        assert_eq!(s, ApiStatus::Other(9999));
        assert_eq!(s.code(), 9999);
        assert!(!s.is_success());
    }

    #[test]
    fn only_transient_codes_are_retryable() {
        assert!(ApiStatus::ServerBusy.is_retryable());
        assert!(ApiStatus::IpRateLimited.is_retryable());
        assert!(!ApiStatus::InvalidApiKey.is_retryable());
        assert!(!ApiStatus::InsufficientCredit.is_retryable());
    }
}
