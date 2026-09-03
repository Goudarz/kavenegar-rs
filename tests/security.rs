//! Security properties that must not regress.
//!
//! Kavenegar authenticates by putting the API key in the URL path. That single
//! design fact drives most of what is asserted here: anything that could carry
//! a URL off the happy path — an error message, a redirect, a downgraded
//! scheme — is a place the credential could escape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::{Arc, Mutex};

use kavenegar::{Error, Kavenegar};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const KEY: &str = "SECRETKEY0123456789abcdef";

/// A stub that replies with whatever raw HTTP the test hands it.
async fn raw_server(response: impl Into<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let response: Arc<String> = Arc::new(response.into());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 2048];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            recorded
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).into_owned());
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    (format!("http://{addr}"), seen)
}

/// Wraps a body in a well-formed HTTP/1.1 response.
fn http_response(status_line: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn client_for(base: &str) -> Kavenegar {
    Kavenegar::builder(KEY).base_url(base).build().unwrap()
}

/// Renders an error every way an application might, including its full source
/// chain, so nothing can hide a leak behind a formatting choice.
fn render_every_way(err: &Error) -> String {
    let mut out = format!("{err} {err:?}");
    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(err);
    while let Some(e) = source {
        out.push_str(&format!(" {e} {e:?}"));
        source = e.source();
    }
    out
}

/// The key must not appear in a transport error, in any of its forms.
///
/// `reqwest` embeds the request URL in its own error messages, so passing that
/// error through would put the credential into every log line that formats it.
#[tokio::test]
async fn a_transport_error_never_carries_the_api_key() {
    // Port 9 (discard) refuses connections, giving a guaranteed transport error.
    let client = client_for("http://127.0.0.1:9");
    let err = client.sms().send(["09120000000"], "x").await.unwrap_err();

    let rendered = render_every_way(&err);
    assert!(!rendered.contains(KEY), "api key leaked: {rendered}");
}

/// The key must not survive into an error raised from a response body either.
#[tokio::test]
async fn a_decode_error_never_carries_the_api_key() {
    // A body that echoes the key back, which a hostile endpoint could do.
    let body = format!(r#"{{"nope":"{KEY}"}}"#);
    let (base, _) = raw_server(http_response("200 OK", &body)).await;

    let err = client_for(&base)
        .sms()
        .send(["09120000000"], "x")
        .await
        .unwrap_err();

    let rendered = render_every_way(&err);
    assert!(!rendered.contains(KEY), "api key leaked: {rendered}");
    assert!(
        rendered.contains("REDACTED"),
        "expected the echoed key to be redacted: {rendered}"
    );
}

/// A redirect must not be followed: the key is in the path, so the `Location`
/// host would receive it.
#[tokio::test]
async fn redirects_are_not_followed() {
    let (base, seen) = raw_server(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/stolen\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;

    let err = client_for(&base)
        .sms()
        .send(["09120000000"], "x")
        .await
        .unwrap_err();

    // One request only — the redirect was not chased.
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(
        matches!(err, Error::UnexpectedStatus { status: 302, .. }),
        "expected the redirect to surface as a status, got {err:?}"
    );
}

/// Plain HTTP to a remote host would put the key on the wire in clear text.
#[test]
fn plain_http_to_a_remote_host_is_refused() {
    let err = Kavenegar::builder(KEY)
        .base_url("http://api.kavenegar.com/v1")
        .build()
        .unwrap_err();

    assert!(
        matches!(
            err,
            Error::InvalidParameter {
                parameter: "base_url",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn https_and_loopback_http_are_both_accepted() {
    assert!(
        Kavenegar::builder(KEY)
            .base_url("https://api.kavenegar.com/v1")
            .build()
            .is_ok()
    );
    // Loopback stays usable so tests and local mocks work.
    for local in [
        "http://127.0.0.1:8080",
        "http://localhost:3000",
        "http://[::1]:9000",
    ] {
        assert!(
            Kavenegar::builder(KEY).base_url(local).build().is_ok(),
            "{local} should be allowed"
        );
    }
}

#[test]
fn a_non_http_scheme_is_refused() {
    for bad in ["ftp://example.com", "file:///etc/passwd", "not a url"] {
        assert!(
            Kavenegar::builder(KEY).base_url(bad).build().is_err(),
            "{bad} should be refused"
        );
    }
}

/// An oversized body must be refused rather than buffered into memory.
#[tokio::test]
async fn an_oversized_response_body_is_refused() {
    // Claims a gigabyte, so the length check trips before anything is read.
    let (base, _) =
        raw_server("HTTP/1.1 200 OK\r\nContent-Length: 1073741824\r\nConnection: close\r\n\r\n")
            .await;

    let err = client_for(&base)
        .sms()
        .send(["09120000000"], "x")
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::ResponseTooLarge { .. }),
        "expected a size refusal, got {err:?}"
    );
}

/// A chunked body has no declared length, so only the streaming cap can stop
/// it. This is the case that could actually exhaust memory.
#[tokio::test]
async fn an_unbounded_chunked_body_is_capped_while_streaming() {
    const CHUNK: usize = 64 * 1024;

    let mut response =
        String::from("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
    let payload = "x".repeat(CHUNK);
    // Comfortably past the 8 MiB ceiling.
    for _ in 0..160 {
        response.push_str(&format!("{CHUNK:x}\r\n{payload}\r\n"));
    }
    response.push_str("0\r\n\r\n");

    let (base, _) = raw_server(response).await;
    let err = client_for(&base)
        .sms()
        .send(["09120000000"], "x")
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::ResponseTooLarge { .. }),
        "expected the stream to be capped, got {err:?}"
    );
}

/// `Debug` on the client must not print the credential.
#[test]
fn debug_output_redacts_the_key() {
    let client = Kavenegar::new(KEY);
    let rendered = format!("{client:?}");
    assert!(!rendered.contains(KEY), "{rendered}");

    let builder = Kavenegar::builder(KEY);
    let rendered = format!("{builder:?}");
    assert!(!rendered.contains(KEY), "{rendered}");
}
