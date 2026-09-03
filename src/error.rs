//! Error types returned by this crate.

use std::fmt;

/// Convenient alias for fallible operations in this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Anything that can go wrong while talking to the Kavenegar API.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The API answered, but its `return` envelope carried a failure status.
    #[error(transparent)]
    Api(#[from] ApiError),

    /// The request never produced a usable HTTP response.
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// The body arrived but could not be decoded into the expected shape.
    ///
    /// The raw `body` is retained so callers can log it if the API changes.
    #[error("could not decode response body: {source}")]
    Decode {
        /// The underlying serde failure.
        #[source]
        source: serde_json::Error,
        /// The raw response body.
        body: String,
    },

    /// The server answered with an error status and a body that was not the
    /// usual JSON envelope — typically a proxy or gateway page.
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

    /// A parameter was rejected locally, before any request was sent.
    #[error("invalid parameter `{parameter}`: {reason}")]
    InvalidParameter {
        /// Name of the offending parameter.
        parameter: &'static str,
        /// Why it was rejected.
        reason: String,
    },
}

impl Error {
    pub(crate) fn invalid(parameter: &'static str, reason: impl Into<String>) -> Self {
        Error::InvalidParameter {
            parameter,
            reason: reason.into(),
        }
    }

    /// The API status code, when this error came from the API envelope.
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
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Api(e) => e.status().is_retryable(),
            Error::Http(e) => e.is_timeout() || e.is_connect(),
            Error::UnexpectedStatus { status, .. } => *status >= 500,
            _ => false,
        }
    }
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
    pub fn status(&self) -> ApiStatus {
        self.status
    }

    /// The numeric status code, including codes this crate does not name.
    pub fn code(&self) -> i32 {
        self.status.code()
    }

    /// The message Kavenegar sent, verbatim (usually Persian).
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
    InvalidCharacters = 422, "the message contains characters the line cannot carry";
    TemplateNotFound = 424, "no verification template with that name exists";
    ServicePlanRequired = 426, "this method requires the advanced service plan";
    LineAccessRequired = 427, "using this line requires an access level to be granted";
    TokenNotDialable = 428, "a voice call cannot read out a non-numeric token";
    IpRateLimited = 429, "too many requests from this IP";
    AccountNotVerified = 430, "the account has not completed identity verification";
    InvalidCodeStructure = 431, "the request structure is invalid";
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
    pub fn is_success(self) -> bool {
        self == ApiStatus::Ok
    }

    /// Whether retrying the identical request could plausibly succeed.
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
