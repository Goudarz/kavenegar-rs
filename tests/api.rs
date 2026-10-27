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
async fn unblocking_is_a_delete_with_a_query_and_returns_the_message() {
    // The documented response: `entries` is an object, not a list.
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":{"message":"Done-removed 1 item from blacklist"}}"#,
    )
    .await;

    let done = stub
        .client()
        .line()
        .unblock("10002263", ["09121234567"])
        .await
        .unwrap();
    assert_eq!(done.message, "Done-removed 1 item from blacklist");

    let request = stub.last_request();
    assert!(
        request.starts_with(
            "DELETE /test-key/line/blocked/remove.json?linenumber=10002263&receptor=09121234567 "
        ),
        "{request}"
    );
}

#[tokio::test]
async fn the_existence_check_is_a_get() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":[{"linenumber":"123456","receptor":"09224445555","status":"NotExists"}]}"#,
    )
    .await;

    let found = stub
        .client()
        .line()
        .is_blocked("123456", ["09224445555"])
        .await
        .unwrap();
    assert_eq!(found[0].status, BlockedState::NotExists);

    let request = stub.last_request();
    assert!(
        request.starts_with(
            "GET /test-key/line/blocked/exists.json?linenumber=123456&receptor=09224445555 "
        ),
        "{request}"
    );
}

#[tokio::test]
async fn media_lookups_use_the_documented_verbs() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"ok"},
            "entries":{"id":"3fa85f64-5717-4562-b3fc-2c963f66afa6","deleted":true}}"#,
    )
    .await;
    let client = stub.client();

    client.media().get("3fa85f64").await.unwrap();
    assert!(
        stub.last_request()
            .starts_with("GET /test-key/media/get.json?id=3fa85f64 "),
        "{}",
        stub.last_request()
    );

    let gone = client.media().delete("3fa85f64").await.unwrap();
    assert!(gone.deleted);
    assert!(
        stub.last_request()
            .starts_with("DELETE /test-key/media/delete.json?id=3fa85f64 "),
        "{}",
        stub.last_request()
    );
}

#[tokio::test]
async fn reading_the_config_is_a_get_and_decodes_the_documented_payload() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":{"apilogs":"justfaults","dailyreport":"disabled","debugmode":"enabled",
                       "defaultsender":"10004535","mincreditalarm":"50000","resendfailed":"true"}}"#,
    )
    .await;

    let config = stub.client().account().config().await.unwrap();
    assert_eq!(config.min_credit_alarm, 50000);
    assert!(
        stub.last_request()
            .starts_with("GET /test-key/account/config.json "),
        "{}",
        stub.last_request()
    );
}

#[tokio::test]
async fn adding_a_template_decodes_the_documented_response() {
    // addtemplate answers with lowercase names and the id as a string.
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":{"templateid":"12345","name":"LoginOtp","sourcetype":"0",
                       "sendmethod":"1","fallbackmethod":"2","primarylinenumber":"10004346",
                       "secondarylinenumber":"1000596446","switchttL":2,"sourceurl":null,
                       "sourcename":null,"textmessage":"کد ورود شما %token است",
                       "voicemessage":"کد ورود شما %token است"}}"#,
    )
    .await;

    let template = stub
        .client()
        .verify()
        .add_template(
            "LoginOtp",
            kavenegar::SourceType::Website,
            kavenegar::SendMethod::Sms,
        )
        .text_message("کد ورود شما %token است")
        .await
        .unwrap();

    assert_eq!(template.id, 12345);
    assert_eq!(template.primary_line_number, "10004346");
    assert_eq!(template.switch_ttl, 2);
}

#[tokio::test]
async fn send_array_posts_json_arrays_and_documented_names() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},"entries":[]}"#,
    )
    .await;

    stub.client()
        .sms()
        .send_array(["09121111111", "09122222222"], ["a", "b"])
        .sender("30002626")
        .local_ids([10, 11])
        .await
        .unwrap();

    let request = stub.last_request();
    assert!(
        request.starts_with("POST /test-key/sms/sendarray.json "),
        "{request}"
    );
    // Form-encoded `["09121111111","09122222222"]` and `[10,11]`.
    assert!(
        request.contains("receptor=%5B%2209121111111%22%2C%2209122222222%22%5D"),
        "{request}"
    );
    assert!(
        request.contains("localmessageids=%5B10%2C11%5D"),
        "{request}"
    );
}

#[tokio::test]
async fn inbox_records_carry_no_delivery_status() {
    let stub = Stub::start(
        "200 OK",
        r#"{"return":{"status":200,"message":"تایید شد"},
            "entries":[{"messageid":35850015,"message":"سلام","sender":"09121234567",
                        "receptor":"3000202030","date":1357206241}],
            "metadata":{"totalcount":"1","currentpage":"1","totalpages":"1","pagesize":"200"}}"#,
    )
    .await;

    let page = stub
        .client()
        .sms()
        .inbox_paged("3000202030")
        .unread()
        .await
        .unwrap();

    assert_eq!(page.len(), 1);
    assert_eq!(page.page_size, 200);
    assert_eq!(page[0].status, MessageStatus::Unknown);
    assert_eq!(page.next_page(), None);
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
