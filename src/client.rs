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
use serde_json::value::RawValue;

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

use crate::error::{ApiError, ApiStatus, Error, Result, printable, redact};
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
    /// Panics if the API key is empty or malformed (see
    /// [`ClientBuilder::build`]), or if the TLS backend cannot be
    /// initialised. Use [`Kavenegar::builder`] to handle those as a `Result`.
    #[expect(
        clippy::expect_used,
        reason = "documented panic; Kavenegar::builder returns a Result for callers who need one"
    )]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::builder(api_key)
            .build()
            .expect("could not build the Kavenegar client")
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
    #[must_use]
    pub fn sms(&self) -> SmsService<'_> {
        SmsService::new(self)
    }

    /// One-time passwords and notification templates.
    #[cfg(feature = "verify")]
    #[must_use]
    pub fn verify(&self) -> VerifyService<'_> {
        VerifyService::new(self)
    }

    /// Text-to-speech voice calls.
    #[cfg(feature = "call")]
    #[must_use]
    pub fn call(&self) -> CallService<'_> {
        CallService::new(self)
    }

    /// Media files usable by messenger lines.
    #[cfg(feature = "media")]
    #[must_use]
    pub fn media(&self) -> MediaService<'_> {
        MediaService::new(self)
    }

    /// Server time, and other utilities.
    #[cfg(feature = "utils")]
    #[must_use]
    pub fn utils(&self) -> UtilsService<'_> {
        UtilsService::new(self)
    }

    /// Credit, expiry and account settings.
    #[cfg(feature = "account")]
    #[must_use]
    pub fn account(&self) -> AccountService<'_> {
        AccountService::new(self)
    }

    /// The blocked-recipient list.
    #[cfg(feature = "line")]
    #[must_use]
    pub fn line(&self) -> LineService<'_> {
        LineService::new(self)
    }

    /// The API key this client authenticates with. Treat the result as a
    /// secret: do not log it.
    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.inner.api_key
    }

    /// The API root this client targets.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// Runs a request whose `entries` is a list, mapping a missing or null
    /// `entries` to an empty vector.
    #[cfg(any(feature = "sms", feature = "call", feature = "line"))]
    pub(crate) async fn call_list<T: DeserializeOwned>(
        &self,
        verb: Verb,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<Vec<T>> {
        let (status, body) = self.fetch(verb, module, method, params).await?;
        let envelope = self.envelope(status, &body)?;
        match envelope.entries {
            None => Ok(Vec::new()),
            Some(raw) => self.decode_list(raw, &body),
        }
    }

    /// Runs a request whose `entries` is a single object.
    #[cfg(any(
        feature = "sms",
        feature = "verify",
        feature = "account",
        feature = "line",
        feature = "media",
        feature = "utils"
    ))]
    pub(crate) async fn call_one<T: DeserializeOwned>(
        &self,
        verb: Verb,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<T> {
        let (status, body) = self.fetch(verb, module, method, params).await?;
        let envelope = self.envelope(status, &body)?;
        self.decode_single(method, envelope.entries, &body)
    }

    /// Runs a request whose payload is not needed, only its success.
    #[cfg(feature = "verify")]
    pub(crate) async fn call_discard(
        &self,
        verb: Verb,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<()> {
        let (status, body) = self.fetch(verb, module, method, params).await?;
        self.envelope(status, &body).map(|_| ())
    }

    /// Runs a paginated request, pairing the records with the `metadata`
    /// counters needed to fetch the next page.
    #[cfg(any(feature = "sms", feature = "verify", feature = "line"))]
    pub(crate) async fn call_page<T: DeserializeOwned>(
        &self,
        verb: Verb,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<Page<T>> {
        let (status, body) = self.fetch(verb, module, method, params).await?;
        let envelope = self.envelope(status, &body)?;

        let items: Vec<T> = match envelope.entries {
            None => Vec::new(),
            Some(raw) => self.decode_list(raw, &body)?,
        };

        let meta: PageMeta = match envelope.metadata {
            Some(raw) => serde_json::from_str(raw.get())
                .map_err(|source| self.decode_error(source, &body))?,
            // Absent metadata is not an error: it just means one page.
            None => {
                let counted = i64::try_from(items.len()).unwrap_or(i64::MAX);
                PageMeta {
                    total_count: counted,
                    current_page: 1,
                    total_pages: 1,
                    page_size: counted,
                }
            }
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
            .map_err(|e| Error::transport(&e, &self.inner.api_key))?;
        let (status, body) = self.read_response(response).await?;
        let envelope = self.envelope(status, &body)?;
        self.decode_single(method, envelope.entries, &body)
    }

    /// Builds the URL for one endpoint.
    fn endpoint(&self, module: &str, method: &str) -> String {
        format!(
            "{}/{}/{}/{}.json",
            self.inner.base_url, self.inner.api_key, module, method
        )
    }

    /// Sends one request and reads its body.
    async fn fetch(
        &self,
        verb: Verb,
        module: &str,
        method: &str,
        params: Vec<(&'static str, String)>,
    ) -> Result<(reqwest::StatusCode, String)> {
        let url = self.endpoint(module, method);
        let http = &self.inner.http;

        // POST carries a form body, which is where message text belongs:
        // Persian text and long messages do not survive a query string
        // reliably. GET and DELETE are used only where Kavenegar documents
        // that verb, and their parameters are short identifiers.
        let request = match verb {
            Verb::Post => http.post(&url).form(&params),
            Verb::Get => http.get(&url).query(&params),
            Verb::Delete => http.delete(&url).query(&params),
        };
        let response = request
            .send()
            .await
            .map_err(|e| Error::transport(&e, &self.inner.api_key))?;
        self.read_response(response).await
    }

    /// Reads a response body, refusing to buffer more than [`MAX_RESPONSE_BYTES`].
    ///
    /// A compromised or misbehaving endpoint should not be able to exhaust the
    /// caller's memory, so the length is enforced while streaming rather than
    /// trusting `Content-Length`.
    async fn read_response(
        &self,
        mut response: reqwest::Response,
    ) -> Result<(reqwest::StatusCode, String)> {
        let too_large = || Error::ResponseTooLarge {
            limit: MAX_RESPONSE_BYTES,
        };

        let status = response.status();
        let declared = response.content_length();
        if declared.is_some_and(|n| n > MAX_RESPONSE_BYTES as u64) {
            return Err(too_large());
        }

        // Kavenegar declares the length, so this is usually one allocation.
        let capacity = declared.and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
        let mut buf: Vec<u8> = Vec::with_capacity(capacity);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::transport(&e, &self.inner.api_key))?
        {
            if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(too_large());
            }
            buf.extend_from_slice(&chunk);
        }

        // Valid UTF-8 — every Kavenegar response — is taken without a copy.
        let body = String::from_utf8(buf)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
        Ok((status, body))
    }

    /// Parses the `return` envelope and fails on a non-success status.
    ///
    /// The payload is borrowed from `body` rather than parsed into an
    /// intermediate tree, so each response is decoded in a single pass.
    fn envelope<'b>(&self, status: reqwest::StatusCode, body: &'b str) -> Result<Envelope<'b>> {
        let envelope: Envelope<'b> = match serde_json::from_str(body) {
            Ok(envelope) => envelope,
            Err(source) => {
                // A non-JSON body from an error status is almost always a
                // proxy or gateway page; report that rather than a serde error.
                if !status.is_success() {
                    return Err(Error::UnexpectedStatus {
                        status: status.as_u16(),
                        body: self.sanitize(body),
                    });
                }
                return Err(self.decode_error(source, body));
            }
        };

        let api_status = ApiStatus::from(envelope.ret.status);
        if !api_status.is_success() {
            // The message is server-supplied text bound for logs.
            let message = printable(&envelope.ret.message, MAX_API_MESSAGE_CHARS);
            return Err(ApiError::new(api_status, message).into());
        }
        Ok(envelope)
    }

    /// Decodes an `entries` value holding a list. A bare object, which some
    /// endpoints send for a single result, becomes a one-element list.
    #[cfg(any(
        feature = "sms",
        feature = "verify",
        feature = "call",
        feature = "line"
    ))]
    fn decode_list<T: DeserializeOwned>(&self, raw: &RawValue, body: &str) -> Result<Vec<T>> {
        let json = raw.get();
        if is_json_array(json) {
            serde_json::from_str(json).map_err(|source| self.decode_error(source, body))
        } else {
            serde_json::from_str(json)
                .map(|one| vec![one])
                .map_err(|source| self.decode_error(source, body))
        }
    }

    /// Decodes an `entries` value that should hold exactly one record.
    ///
    /// Some endpoints send that record as a bare object and others wrap it in
    /// a one-element array, so both are accepted.
    #[cfg(any(
        feature = "sms",
        feature = "verify",
        feature = "account",
        feature = "line",
        feature = "media",
        feature = "utils"
    ))]
    fn decode_single<T: DeserializeOwned>(
        &self,
        method: &str,
        entries: Option<&RawValue>,
        body: &str,
    ) -> Result<T> {
        let Some(raw) = entries else {
            return Err(Error::MissingEntries {
                method: method.to_owned(),
                body: self.sanitize(body),
            });
        };
        let json = raw.get();
        if is_json_array(json) {
            let mut items: Vec<T> =
                serde_json::from_str(json).map_err(|source| self.decode_error(source, body))?;
            if items.len() == 1
                && let Some(one) = items.pop()
            {
                return Ok(one);
            }
        }
        serde_json::from_str(json).map_err(|source| self.decode_error(source, body))
    }

    /// Builds a decode error, cleaning the body only now that it is needed.
    fn decode_error(&self, source: serde_json::Error, body: &str) -> Error {
        Error::Decode {
            source,
            body: self.sanitize(body),
        }
    }

    /// Trims a body for an error and strips the API key, in case the server
    /// ever echoes it back.
    fn sanitize(&self, body: &str) -> String {
        redact(&truncate(body), &self.inner.api_key)
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

/// The HTTP verb an endpoint is called with.
///
/// Kavenegar's older endpoints accept GET or POST interchangeably; the newer
/// ones (`line/blocked/*`, `media/*`, reading `account/config`) document one
/// specific verb, and that verb is used for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verb {
    #[cfg_attr(
        not(any(feature = "account", feature = "line", feature = "media")),
        allow(dead_code)
    )]
    Get,
    // `media` alone posts only through the multipart upload path.
    #[cfg_attr(
        not(any(
            feature = "sms",
            feature = "verify",
            feature = "call",
            feature = "account",
            feature = "line",
            feature = "utils"
        )),
        allow(dead_code)
    )]
    Post,
    #[cfg_attr(not(any(feature = "line", feature = "media")), allow(dead_code))]
    Delete,
}

/// Whether a raw JSON value is an array. `RawValue` holds no surrounding
/// whitespace, so the first byte decides.
#[cfg(any(
    feature = "sms",
    feature = "verify",
    feature = "call",
    feature = "account",
    feature = "line",
    feature = "media",
    feature = "utils"
))]
fn is_json_array(json: &str) -> bool {
    json.trim_start().starts_with('[')
}

/// The longest server-supplied error message kept, in characters.
const MAX_API_MESSAGE_CHARS: usize = 512;

/// The response envelope every Kavenegar endpoint wraps its payload in.
///
/// `entries` and `metadata` stay as unparsed slices of the body until the
/// caller knows what type they hold. A JSON `null` reads as `None`.
#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(rename = "return")]
    ret: ReturnBlock,
    #[serde(default, borrow)]
    #[cfg_attr(
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
    entries: Option<&'a RawValue>,
    #[serde(default, borrow)]
    #[cfg_attr(
        not(any(feature = "sms", feature = "verify", feature = "line")),
        allow(dead_code)
    )]
    metadata: Option<&'a RawValue>,
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
#[must_use = "a builder does nothing until `build` is called"]
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
            .field("custom_http_client", &self.http.is_some())
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
    ///
    /// # Security
    ///
    /// That client is used as-is, so the protections this crate sets on its
    /// own client do not apply to it. In particular, `reqwest` follows
    /// redirects by default, and a redirect hands the request path — which
    /// contains the API key — to whatever host it names. Build yours with
    /// `.redirect(reqwest::redirect::Policy::none())`.
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Builds the client.
    ///
    /// Surrounding whitespace on the API key, such as the trailing newline of
    /// a secrets file, is removed first.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`] if the API key is empty or holds a
    /// character that would change the request URL — inner whitespace, `/`,
    /// `?`, `#` or `%` — or if the base URL is not HTTPS. Returns
    /// [`Error::Http`] if the TLS backend fails to initialise.
    pub fn build(self) -> Result<Kavenegar> {
        let api_key = self.api_key.trim().to_owned();
        crate::check::api_key(&api_key)?;

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
                    .connect_timeout(Duration::from_secs(10))
                    // Defence in depth behind `check_base_url`: once the root
                    // is HTTPS, no request from this client can go out in
                    // clear text.
                    .https_only(base_url.starts_with("https://"));
                // rustls already refuses anything older, but a platform TLS
                // stack may still offer TLS 1.0 and 1.1.
                #[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
                {
                    builder = builder.tls_version_min(reqwest::tls::Version::TLS_1_2);
                }
                if let Some(timeout) = self.timeout {
                    builder = builder.timeout(timeout);
                }
                builder
                    .build()
                    .map_err(|e| Error::transport(&e, &api_key))?
            }
        };

        Ok(Kavenegar {
            inner: Arc::new(Inner {
                api_key,
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

    // Endpoint paths are appended to the base URL as text, so a query or
    // fragment would swallow them.
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::invalid(
            "base_url",
            "must not contain a query string or fragment",
        ));
    }

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
            "refusing plain HTTP to a non-loopback host: the API key travels \
             in the URL path and would be sent in clear text",
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
    fn surrounding_whitespace_on_the_key_is_trimmed() {
        let client = Kavenegar::builder(
            "  abc123
",
        )
        .build()
        .unwrap();
        assert_eq!(client.api_key(), "abc123");
        assert!(Kavenegar::builder("abc 123").build().is_err());
    }

    #[test]
    fn a_base_url_with_a_query_is_refused() {
        assert!(
            Kavenegar::builder("k")
                .base_url("https://api.kavenegar.com/v1?x=1")
                .build()
                .is_err()
        );
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
