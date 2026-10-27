# Security policy

## Reporting a vulnerability

Please report security issues privately to **hi@goudarzjafari.com**, or through
GitHub's [private vulnerability reporting][gh] on this repository. Do not open
a public issue for a vulnerability.

Include the crate version, a description of the problem, and a reproduction if
you have one. You can expect an initial response within a week.

[gh]: https://github.com/goudarz/kavenegar-rs/security/advisories/new

## Supported versions

While the crate is below `1.0.0`, only the latest published version receives
security fixes.

## Threat model

This crate is an HTTP client. It holds a credential, talks to one remote
service over TLS, and parses that service's replies. The security properties
below follow from that, and each is covered by a test in
[`tests/security.rs`](tests/security.rs).

### The API key is the asset

Kavenegar authenticates by putting the API key **in the URL path**:

```text
https://api.kavenegar.com/v1/{API-KEY}/sms/send.json
```

That design decision is the source of most of the risk here, because a URL
travels further than a header does. The crate therefore:

- **Never lets the URL reach an error message.** `reqwest` embeds the request
  URL in its own errors, so passing those through would write the key into any
  log that formats the error. Transport failures are re-wrapped as
  [`TransportError`], which keeps the diagnosis (timeout, refused connection,
  TLS failure) and drops the URL.
- **Redacts the key from response bodies** kept for diagnostics, in case the
  server ever echoes it back.
- **Redacts the key in `Debug`** for both `Kavenegar` and `ClientBuilder`.
- **Validates the key's characters.** The key is placed in the URL path
  verbatim, so whitespace (a trailing newline from a secrets file), `/`, `?`,
  `#` or `%` would silently change which URL is requested. Such keys are
  rejected when the client is built, without echoing the key.
- **Requires TLS 1.2 or newer, and HTTPS only.** rustls never offers older
  versions; the platform stack (`native-tls`) might, so the floor is set
  explicitly. Once the base URL is HTTPS the client also refuses to send any
  request over plain HTTP.
- **Refuses plain HTTP** to any non-loopback host, so the key cannot be sent
  in clear text by a misconfigured `base_url`. Loopback stays allowed so local
  mock servers work.
- **Does not follow redirects.** A `Location` header would otherwise hand the
  path — and the key in it — to a host of the server's choosing. This applies
  to the client the crate builds; a client you supply through
  `ClientBuilder::http_client` is used as-is, so configure it with
  `redirect::Policy::none()` yourself.

What this crate cannot do for you: the key is still in the URL on the wire
inside the TLS session, and it will appear in Kavenegar's own server logs and
in any proxy you configure. Treat a corporate TLS-terminating proxy as a party
that learns your key.

### Abuse through your own users: SMS pumping

Kavenegar takes recipient lists as comma-separated text. An application that
passes a phone number typed by an end user straight into `send` or `lookup`
would let that user enter `"0912…,0913…,…"` and turn one request into up to
200 sends billed to the application's account — the attack known as SMS
pumping or toll fraud. Every phone and line number is therefore checked to be
plain ASCII digits with an optional leading `+` before anything is sent.

This closes the injection; it does not rate-limit. Throttle OTP requests per
user and per IP in your application as well.

### Secrets and personal data in `Debug` output

Besides the client's own key, `Debug` redacts a sub-account's API key on the
template builders, the token values of a lookup (usually one-time passwords),
and the recipients and text of `send`, `send_array` and `text_to_speech`,
showing only counts and lengths. An upload builder shows the file's size, not
its bytes.

### Log injection

Kavenegar's error message (`return.message`) is text from the network that
ends up in `Display` and therefore in logs. Before it is stored, control
characters — newlines that could forge extra log entries, ANSI escapes that
could rewrite a terminal — and Unicode direction overrides become spaces, and
it is capped at 512 characters.

`Error::Decode`'s `Display` names only the kind and position of the failure.
A serde message can quote the value it rejected — a phone number, a message
body — so that full message is reachable only through `source()` or `Debug`.

### Retries and duplicates

`reqwest` 0.13 can retry on its own. Its default, which this crate keeps,
retries only a request the server explicitly refused before processing it
(an HTTP/2 `REFUSED_STREAM` or `GOAWAY`), so it cannot duplicate a send.


`Error::is_retryable` reports timeouts as retryable, but a timed-out send may
already have been accepted. Retrying it can deliver twice and charge twice.
Attach `local_ids` — Kavenegar refuses to send a repeated local id — before
retrying a send, and issue a fresh one-time password rather than resending.

### Denial of service

- Response bodies are capped at [`MAX_RESPONSE_BYTES`] (8 MiB) **while
  streaming**, not by trusting `Content-Length`, so a chunked reply with no
  declared length cannot exhaust memory.
- A 30-second overall timeout and a 10-second connect timeout are set by
  default.
- Batch-size limits documented by Kavenegar are enforced locally: 200
  recipients per send, `send_array`, voice call, block or existence check; 50
  per unblock; 500 message ids per status, select or cancel; and matching
  array lengths for `send_array`. Uploads over the documented 10 MB (image)
  or 20 MB (video) limits are refused before they are sent.

### Personal data in errors

`Error::Decode`, `Error::UnexpectedStatus` and `Error::MissingEntries` retain
the response body so you can diagnose an API change. **That body routinely
contains recipient phone numbers, and may contain message text — including a
one-time password you just sent.**

The body is never included in the `Display` output, so `error!("{e}")` is safe.
It *is* included in `Debug`, so treat `error!("{e:?}")` as a log line
containing personal data, and apply whatever retention rules you apply to the
messages themselves.

### Memory safety

- `unsafe_code = "forbid"` at the crate root: this crate contains no `unsafe`.
- `unwrap`, `expect` and `panic!` are denied in library code. The single
  documented exception is [`Kavenegar::new`], which panics if the TLS backend
  fails to start; use `Kavenegar::builder(..).build()` for a `Result` instead.
- Casts that could truncate or wrap are denied; conversions from
  network-supplied values use checked arithmetic.

### What is deliberately not done

- **No zeroing of the key in memory.** The key is copied into every request URL
  by design, so many transient allocations hold it. Zeroing the one stored copy
  would look reassuring while leaving the others untouched, so it is not done.
  If your threat model includes memory disclosure, scope the credential at the
  process level instead.
- **No certificate pinning.** Kavenegar may rotate certificates without notice,
  and a pin that breaks in production is worse than none. Supply your own
  configured [`reqwest::Client`] via `ClientBuilder::http_client` if you want it.
- **No built-in rate limiting or retry.** [`Error::is_retryable`] tells you
  whether a retry could plausibly succeed; the backoff policy is yours, because
  only you know your traffic shape.

## Supply chain

- Dependencies are few and mainstream: `reqwest`, `serde`, `serde_json`,
  `thiserror`. Only the `reqwest` features actually used are enabled, which
  keeps `encoding_rs` and `mime` out of the tree.
- `Cargo.lock` is committed, so CI builds exactly what was reviewed, and
  Dependabot proposes dependency and action updates weekly.
- Every GitHub Action is pinned to a full commit SHA, not a movable tag. The
  workflow token is read-only by default, and checkout does not persist
  credentials.
- CI runs [`cargo audit`](https://github.com/RustSec/rustsec) against the
  [RustSec advisory database](https://rustsec.org) and
  [`cargo deny`](https://github.com/EmbarkStudios/cargo-deny) for advisories,
  licences, and duplicate or banned crates, on every push and weekly.
- `rustls` is the default TLS backend, so no system OpenSSL is required.

To check a build yourself:

```bash
cargo install cargo-audit cargo-deny
cargo audit
cargo deny check
```

[`TransportError`]: https://docs.rs/kavenegar/latest/kavenegar/error/struct.TransportError.html
[`MAX_RESPONSE_BYTES`]: https://docs.rs/kavenegar/latest/kavenegar/client/constant.MAX_RESPONSE_BYTES.html
[`Kavenegar::new`]: https://docs.rs/kavenegar/latest/kavenegar/struct.Kavenegar.html#method.new
[`Error::is_retryable`]: https://docs.rs/kavenegar/latest/kavenegar/enum.Error.html#method.is_retryable
[`reqwest::Client`]: https://docs.rs/reqwest/latest/reqwest/struct.Client.html
