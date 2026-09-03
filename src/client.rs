//! The HTTP client and the request plumbing shared by every service.

#![cfg_attr(
    not(any(
        feature = "sms",
        feature = "verify",
        feature = "call",
        feature = "account",
        feature = "line",
        feature = "media",
        feature = "utils"
    )),
    allow(dead_code)
)]

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

#[cfg(any(
    feature = "sms",
    feature = "verify",
    feature = "call",
    feature = "account",
    feature = "line",
    feature = "media",
    feature = "utils"
))]
use serde::de::DeserializeOwned;

use crate::error::{ApiError, ApiStatus, Error, Result, redact};
#[cfg(any(feature = "sms", feature = "verify", feature = "line"))]
use crate::page::{Page, PageMeta};

#[cfg(feature = "account")]
use crate::account::AccountService;
#[cfg(feature = "call")]
use crate::call::CallService;
#[cfg(feature = "line")]
use crate::line::LineService;
#[cfg(feature = "media")]
use crate::media::MediaService;
#[cfg(feature = "sms")]
use crate::sms::SmsService;
#[cfg(feature = "utils")]
use crate::utils::UtilsService;
#[cfg(feature = "verify")]
use crate::verify::VerifyService;

/// The public Kavenegar API root.
pub const DEFAULT_BASE_URL: &str = "https://api.kavenegar.com/v1";

/// The largest response body this client will buffer, in bytes.
///
/// Kavenegar's own responses are a few kilobytes at most; the ceiling exists
/// so a hostile or broken endpoint cannot exhaust memory.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// A boxed future, returned when awaiting a request builder.
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// An asynchronous Kavenegar API client.
///
/// The client is cheap to clone — clones share one connection pool — so store
/// a single instance in your application state rather than building one per
/// request.
///
/// # Examples
///
/// ```no_run
/// # async fn run() -> Result<(), kavenegar::Error> {
/// let client = kavenegar::Kavenegar::new("your-api-key");
/// let sent = client.sms().send(["09177788155"], "سلام").await?;
/// println!("message id: {}", sent[0].message_id);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Kavenegar {
    inner: Arc<Inner>,
}

struct Inner {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl Kavenegar {
    /// Creates a client with the default configuration.
    ///
    /// # Panics
    ///
    /// Panics if the underlying TLS backend cannot be initialised. Use
    /// [`Kavenegar::builder`] if you need to handle that failure.
    #[expect(
        clippy::expect_used,
        reason = "documented panic; Kavenegar::builder returns a Result for callers who need one"
    )]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::builder(api_key)
            .build()
            .expect("failed to build the default HTTP client")
    }

    /// Starts building a client with custom timeouts, a custom base URL, or
    /// a pre-configured [`reqwest::Client`].
    pub fn builder(api_key: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: Some(Duration::from_secs(30)),
            http: None,
        }
    }

    /// Sending messages, and querying their status and history.
    #[cfg(feature = "sms")]
    pub fn sms(&self) -> SmsService<'_> {
        SmsService::new(self)
    }

    /// One-time passwords and notification templates.
    #[cfg(feature = "verify")]
    pub fn verify(&self) -> VerifyService<'_> {
        VerifyService::new(self)
    }

    /// Text-to-speech voice calls.
    #[cfg(feature = "call")]
    pub fn call(&self) -> CallService<'_> {
        CallService::new(self)
    }

    /// Media files usable by messenger lines.
    #[cfg(feature = "media")]
    pub fn media(&self) -> MediaService<'_> {
        MediaService::new(self)
    }

    /// Server time, and other utilities.
    #[cfg(feature = "utils")]
    pub fn utils(&self) -> UtilsService<'_> {
        UtilsService::new(self)
    }

    /// Credit, expiry and account settings.
    #[cfg(feature = "account")]
    pub fn account(&self) -> AccountService<'_> {
        AccountService::new(self)
    }

    /// The blocked-recipient list.
    #[cfg(feature = "line")]
    pub fn line(&self) -> LineService<'_> {
        LineService::new(self)
    }

    /// The API key this client authenticates with.
    pub fn api_key(&self) -> &str {
        &self.inner.api_key
    }

    /// The API root this client targets.
    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// Runs a request whose `entries` is a list, mapping a missing or null
    /// `entries` to an empty vector.
    #[cfg(any(feature = "sms", feature = "call", feature = "line"))]
    pub(crate) async fn call_list<T: DeserializeOwned>(
        &self,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<Vec<T>> {
        let (entries, _, body) = self.request(module, method, params).await?;
        match entries {
            Value::Null => Ok(Vec::new()),
            Value::Array(_) => {
                serde_json::from_value(entries).map_err(|source| Error::Decode { source, body })
            }
            // Some endpoints collapse a single result to a bare object.
            other => serde_json::from_value(other)
                .map(|one| vec![one])
                .map_err(|source| Error::Decode { source, body }),
        }
    }

    /// Runs a request whose `entries` is a single object.
    #[cfg(any(
        feature = "sms",
        feature = "verify",
        feature = "account",
        feature = "media",
        feature = "utils"
    ))]
    pub(crate) async fn call_one<T: DeserializeOwned>(
        &self,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<T> {
        let (entries, _, body) = self.request(module, method, params).await?;
        let value = match entries {
            // ...and some wrap a single result in a one-element array.
            Value::Array(mut items) if items.len() == 1 => items.remove(0),
            Value::Null => {
                return Err(Error::MissingEntries {
                    method: method.to_owned(),
                    body,
                });
            }
            other => other,
        };
        serde_json::from_value(value).map_err(|source| Error::Decode { source, body })
    }

    /// Runs a request whose payload is not needed, only its success.
    #[cfg(feature = "verify")]
    pub(crate) async fn call_discard(
        &self,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<()> {
        self.request(module, method, params).await.map(|_| ())
    }

    /// Runs a paginated request, pairing the records with the `metadata`
    /// counters needed to fetch the next page.
    #[cfg(any(feature = "sms", feature = "verify", feature = "line"))]
    pub(crate) async fn call_page<T: DeserializeOwned>(
        &self,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<Page<T>> {
        let (entries, metadata, body) = self.request(module, method, params).await?;

        let items: Vec<T> = match entries {
            Value::Null => Vec::new(),
            Value::Array(_) => serde_json::from_value(entries).map_err(|source| Error::Decode {
                source,
                body: body.clone(),
            })?,
            other => vec![
                serde_json::from_value(other).map_err(|source| Error::Decode {
                    source,
                    body: body.clone(),
                })?,
            ],
        };

        // Absent metadata is not an error: it just means one page.
        let meta: PageMeta = if metadata.is_null() {
            let counted = i64::try_from(items.len()).unwrap_or(i64::MAX);
            PageMeta {
                total_count: counted,
                current_page: 1,
                total_pages: 1,
                page_size: counted,
            }
        } else {
            serde_json::from_value(metadata).map_err(|source| Error::Decode { source, body })?
        };

        Ok(meta.into_page(items))
    }

    /// Uploads a file as `multipart/form-data`, returning the decoded entry.
    #[cfg(feature = "media")]
    pub(crate) async fn call_upload<T: DeserializeOwned>(
        &self,
        module: &str,
        method: &str,
        part_name: &'static str,
        part: reqwest::multipart::Part,
    ) -> Result<T> {
        let url = self.endpoint(module, method);
        let form = reqwest::multipart::Form::new().part(part_name, part);

        let response = self
            .inner
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| Error::transport(e, &self.inner.api_key))?;
        let (entries, _, body) = self.unwrap_envelope(response).await?;

        let value = match entries {
            Value::Array(mut items) if items.len() == 1 => items.remove(0),
            Value::Null => {
                return Err(Error::MissingEntries {
                    method: method.to_owned(),
                    body,
                });
            }
            other => other,
        };
        serde_json::from_value(value).map_err(|source| Error::Decode { source, body })
    }

    /// Builds the URL for one endpoint.
    fn endpoint(&self, module: &str, method: &str) -> String {
        format!(
            "{}/{}/{}/{}.json",
            self.inner.base_url, self.inner.api_key, module, method
        )
    }

    /// Performs the call and unwraps the `return` envelope.
    ///
    /// Returns the raw `entries` and `metadata` values alongside the body they
    /// came from, so callers can attach the body to a decode failure.
    async fn request(
        &self,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<(Value, Value, String)> {
        let url = self.endpoint(module, method);

        // POST with a form body: Persian text and long messages do not
        // survive a query string reliably.
        let response = self
            .inner
            .http
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| Error::transport(e, &self.inner.api_key))?;
        self.unwrap_envelope(response).await
    }

    /// Reads a response body, refusing to buffer more than [`MAX_RESPONSE_BYTES`].
    ///
    /// A compromised or misbehaving endpoint should not be able to exhaust the
    /// caller's memory, so the length is enforced while streaming rather than
    /// trusting `Content-Length`.
    async fn read_body_capped(&self, mut response: reqwest::Response) -> Result<String> {
        let too_large = || Error::ResponseTooLarge {
            limit: MAX_RESPONSE_BYTES,
        };

        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
        {
            return Err(too_large());
        }

        let mut buf: Vec<u8> = Vec::new();
        loop {
            let chunk = response
                .chunk()
                .await
                .map_err(|e| Error::transport(e, &self.inner.api_key))?;
            let Some(chunk) = chunk else { break };

            if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(too_large());
            }
            buf.extend_from_slice(&chunk);
        }

        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    /// Trims a body for an error and strips the API key, in case the server
    /// ever echoes it back.
    fn sanitize(&self, body: &str) -> String {
        redact(&truncate(body), &self.inner.api_key)
    }

    /// Checks the `return` block and hands back the payload.
    async fn unwrap_envelope(&self, response: reqwest::Response) -> Result<(Value, Value, String)> {
        let http_status = response.status();
        let body = self.read_body_capped(response).await?;

        let envelope: Envelope = match serde_json::from_str(&body) {
            Ok(envelope) => envelope,
            Err(source) => {
                // A non-JSON body from an error status is almost always a
                // proxy or gateway page; report that rather than a serde error.
                if !http_status.is_success() {
                    return Err(Error::UnexpectedStatus {
                        status: http_status.as_u16(),
                        body: self.sanitize(&body),
                    });
                }
                return Err(Error::Decode {
                    source,
                    body: self.sanitize(&body),
                });
            }
        };

        let status = ApiStatus::from(envelope.ret.status);
        if !status.is_success() {
            return Err(ApiError::new(status, envelope.ret.message).into());
        }

        Ok((envelope.entries, envelope.metadata, self.sanitize(&body)))
    }
}

impl std::fmt::Debug for Kavenegar {
    /// Redacts the API key, so a client in a log or a panic message never
    /// leaks the credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kavenegar")
            .field("base_url", &self.inner.base_url)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

/// The response envelope every Kavenegar endpoint wraps its payload in.
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "return")]
    ret: ReturnBlock,
    #[serde(default)]
    entries: Value,
    #[serde(default)]
    metadata: Value,
}

#[derive(Deserialize)]
struct ReturnBlock {
    #[serde(default)]
    status: i32,
    #[serde(default)]
    message: String,
}

/// Caps a body kept for diagnostics, so an unexpected megabyte of HTML does
/// not end up in an error message.
fn truncate(body: &str) -> String {
    const LIMIT: usize = 2048;
    if body.len() <= LIMIT {
        return body.to_owned();
    }
    let mut end = LIMIT;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes total)", &body[..end], body.len())
}

/// Builds a [`Kavenegar`] client with non-default settings.
pub struct ClientBuilder {
    api_key: String,
    base_url: String,
    timeout: Option<Duration>,
    http: Option<reqwest::Client>,
}

impl std::fmt::Debug for ClientBuilder {
    /// Redacts the API key, matching [`Kavenegar`]'s own `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

impl ClientBuilder {
    /// Sets the total request timeout. Defaults to 30 seconds.
    ///
    /// Ignored when [`ClientBuilder::http_client`] supplies a client, since
    /// that client carries its own configuration.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Removes the request timeout entirely.
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Points the client at a different API root, without the trailing slash.
    ///
    /// Useful for a mock server in tests, or a corporate proxy.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Supplies a pre-configured [`reqwest::Client`], so the connection pool,
    /// proxy and TLS settings can be shared with the rest of your application.
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`] if the API key is empty, and
    /// [`Error::Http`] if the TLS backend fails to initialise.
    pub fn build(self) -> Result<Kavenegar> {
        if self.api_key.trim().is_empty() {
            return Err(Error::invalid("api_key", "must not be empty"));
        }

        let base_url = self.base_url.trim_end_matches('/').to_owned();
        check_base_url(&base_url)?;

        let http = match self.http {
            Some(http) => http,
            None => {
                let mut builder = reqwest::Client::builder()
                    .user_agent(concat!("kavenegar-rs/", env!("CARGO_PKG_VERSION")))
                    // The API key travels in the URL path, so a redirect would
                    // hand it to whatever host the `Location` header names.
                    .redirect(reqwest::redirect::Policy::none())
                    // Bound the handshake separately: without this, an
                    // unreachable host holds the request for the full timeout.
                    .connect_timeout(Duration::from_secs(10));
                if let Some(timeout) = self.timeout {
                    builder = builder.timeout(timeout);
                }
                builder
                    .build()
                    .map_err(|e| Error::transport(e, &self.api_key))?
            }
        };

        Ok(Kavenegar {
            inner: Arc::new(Inner {
                api_key: self.api_key,
                base_url,
                http,
            }),
        })
    }
}

/// Rejects a base URL that would put the API key on the wire in clear text.
///
/// Plain HTTP is allowed only for loopback, so the stub servers used in tests
/// keep working while a real deployment cannot silently downgrade.
fn check_base_url(base_url: &str) -> Result<()> {
    let url = reqwest::Url::parse(base_url)
        .map_err(|e| Error::invalid("base_url", format!("not a valid URL: {e}")))?;

    let loopback = url.host_str().is_some_and(|host| {
        // `host_str` keeps the brackets around an IPv6 literal.
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });

    match url.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        "http" => Err(Error::invalid(
            "base_url",
            "refusing plain HTTP to a non-loopback host: the API key travels              in the URL path and would be sent in clear text",
        )),
        other => Err(Error::invalid(
            "base_url",
            format!("unsupported scheme `{other}`; expected https"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_api_key_is_rejected() {
        let err = Kavenegar::builder("   ").build().unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "api_key",
                ..
            }
        ));
    }

    #[test]
    fn base_url_loses_its_trailing_slash() {
        let client = Kavenegar::builder("k")
            .base_url("http://localhost:1234/")
            .build()
            .unwrap();
        assert_eq!(client.base_url(), "http://localhost:1234");
    }

    #[test]
    fn debug_never_prints_the_api_key() {
        let client = Kavenegar::new("super-secret-key");
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("super-secret-key"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn truncate_keeps_short_bodies_and_caps_long_ones() {
        assert_eq!(truncate("hello"), "hello");
        let long = "ب".repeat(4000);
        let capped = truncate(&long);
        assert!(capped.len() < long.len());
        assert!(capped.contains("bytes total"));
    }
}
