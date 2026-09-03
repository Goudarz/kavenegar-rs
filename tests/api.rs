//! End-to-end tests against a stub HTTP server.
//!
//! These cover the parts most likely to break against the real API: the
//! `return` envelope, and the fact that Kavenegar is inconsistent about
//! whether `entries` is an object, an array, or absent entirely.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::{Arc, Mutex};

use kavenegar::{ApiStatus, BlockedState, Error, Kavenegar, MessageStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A one-shot HTTP server that replies with a canned body and records what it
/// was sent.
struct Stub {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Stub {
    async fn start(status_line: &'static str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let request = read_request(&mut socket).await;
                recorded.lock().unwrap().push(request);

                let response = format!(
                    "HTTP/1.1 {status_line}\r\n\
                     Content-Type: application/json; charset=utf-8\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        Stub {
            base_url: format!("http://{addr}"),
            requests,
        }
    }

    fn client(&self) -> Kavenegar {
        Kavenegar::builder("test-key")
            .base_url(&self.base_url)
            .build()
            .unwrap()
    }

    fn last_request(&self) -> String {
        self.requests.lock().unwrap().last().cloned().unwrap()
    }
}

/// Reads one full HTTP request, honouring `Content-Length` so a body split
/// across packets does not make the test flaky.
async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);

        let text = String::from_utf8_lossy(&raw);
        let Some(head_end) = text.find("\r\n\r\n") else {
            continue;
        };

        let content_length = text[..head_end]
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);

        if raw.len() >= head_end + 4 + content_length {
            break;
        }
    }

    String::from_utf8_lossy(&raw).into_owned()
}

#[tokio::test]
async fn send_parses_entries_and_posts_a_form() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":[{"messageid":85463238,"message":"hi","status":1,
                        "statustext":"در صف ارسال","sender":"10004346",
                        "receptor":"09177788155","date":1770000000,"cost":120}]}"#,
    )
    .await;

    let sent = stub
        .client()
        .sms()
        .send(["09177788155"], "hi")
        .sender("10004346")
        .await
        .unwrap();

    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].message_id, 85463238);
    assert_eq!(sent[0].status, MessageStatus::Queued);
    assert_eq!(sent[0].cost, 120);

    let request = stub.last_request();
    assert!(
        request.starts_with("POST /test-key/sms/send.json "),
        "{request}"
    );
    assert!(request.contains("receptor=09177788155"), "{request}");
    assert!(request.contains("sender=10004346"), "{request}");
}

#[tokio::test]
async fn a_failure_status_becomes_an_api_error() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":418,"message":"اعتبار شما کافی نیست"},"entries":null}"#,
    )
    .await;

    let err = stub
        .client()
        .sms()
        .send(["09177788155"], "hi")
        .await
        .unwrap_err();

    match err {
        Error::Api(ref e) => {
            assert_eq!(e.status(), ApiStatus::InsufficientCredit);
            assert_eq!(e.message(), "اعتبار شما کافی نیست");
        }
        other => panic!("expected an API error, got {other:?}"),
    }
    // Topping up is the caller's job; retrying the same call cannot help.
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn a_null_entries_list_is_empty_not_an_error() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"ok"},"entries":null}"#,
    )
    .await;

    let found = stub.client().sms().latest_outbox().await.unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn lookup_accepts_a_bare_entries_object() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"ok"},
            "entries":{"messageid":1,"status":5,"receptor":"09177788155"}}"#,
    )
    .await;

    let sent = stub
        .client()
        .verify()
        .lookup("09177788155", "verify-login", "48391")
        .await
        .unwrap();

    assert_eq!(sent.message_id, 1);
    assert_eq!(sent.status, MessageStatus::Sent);
}

#[tokio::test]
async fn lookup_also_accepts_entries_wrapped_in_an_array() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"ok"},
            "entries":[{"messageid":2,"status":10,"receptor":"09177788155"}]}"#,
    )
    .await;

    let sent = stub
        .client()
        .verify()
        .lookup("09177788155", "verify-login", "48391")
        .await
        .unwrap();

    assert_eq!(sent.message_id, 2);
    assert!(sent.status.is_delivered());
}

#[tokio::test]
async fn a_gateway_page_is_reported_as_an_unexpected_status() {
    let stub = Stub::start("502 Bad Gateway", "<html><body>Bad Gateway</body></html>").await;

    let err = stub.client().account().info().await.unwrap_err();

    match err {
        Error::UnexpectedStatus { status, ref body } => {
            assert_eq!(status, 502);
            assert!(body.contains("Bad Gateway"));
        }
        other => panic!("expected an unexpected-status error, got {other:?}"),
    }
    // A 502 is worth retrying.
    assert!(err.is_retryable());
}

#[tokio::test]
async fn blocking_posts_to_the_line_scoped_path() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":[{"linenumber":"10004346","receptor":"09177788155","status":"Active"},
                       {"linenumber":"10004346","receptor":"09131234567","status":"AlreadyExists"}]}"#,
    )
    .await;

    let results = stub
        .client()
        .line()
        .block("10004346", ["09177788155", "09131234567"])
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].status, BlockedState::Active);
    assert_eq!(results[1].status, BlockedState::AlreadyExists);
    assert!(results[1].status.is_blocked());

    // The blocked endpoints sit under `line/blocked`, not `blocked`, and
    // `linenumber` is mandatory.
    let request = stub.last_request();
    assert!(
        request.starts_with("POST /test-key/line/blocked/add.json "),
        "{request}"
    );
    assert!(request.contains("linenumber=10004346"), "{request}");
}

#[tokio::test]
async fn template_listing_uses_templatelist() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"ok"},
            "entries":[{"id":12345,"name":"LoginOtp","smsMessage":"کد ورود شما %token است",
                        "primaryLineNumber":"10004346","sendPriority":"SMS",
                        "switchTTL":2,"approvalStatus":"Approved"}]}"#,
    )
    .await;

    let templates = stub.client().verify().templates().page(1).await.unwrap();

    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].id, 12345);
    assert_eq!(templates[0].name, "LoginOtp");
    assert_eq!(templates[0].send_priority, "SMS");
    assert_eq!(templates[0].switch_ttl, 2);

    let request = stub.last_request();
    assert!(
        request.starts_with("POST /test-key/verify/templatelist.json "),
        "{request}"
    );
}

#[tokio::test]
async fn an_unknown_status_code_still_surfaces_as_an_api_error() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":9999,"message":"something new"},"entries":null}"#,
    )
    .await;

    let err = stub.client().account().info().await.unwrap_err();

    assert_eq!(err.api_status(), Some(ApiStatus::Other(9999)));
}
