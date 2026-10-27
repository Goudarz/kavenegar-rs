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

### Denial of service

- Response bodies are capped at [`MAX_RESPONSE_BYTES`] (8 MiB) **while
  streaming**, not by trusting `Content-Length`, so a chunked reply with no
  declared length cannot exhaust memory.
- A 30-second overall timeout and a 10-second connect timeout are set by
  default.
- Batch-size limits documented by Kavenegar are enforced locally: 200
  recipients per send, `send_array`, voice call, block or existence check; 50
  per unblock; 500 message ids per status, select or cancel; and matching
  array lengths for `send_array`.

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
  `thiserror`.
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
