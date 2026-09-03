//! Tests that talk to the real Kavenegar API.
//!
//! These are `#[ignore]`d, so `cargo test` never runs them by accident — the
//! sending ones cost real credit.
//!
//! # Running them
//!
//! ```text
//! export KAVENEGAR_API_KEY=your-key
//!
//! # Read-only checks. Free, and the right first step.
//! cargo test --test live -- --ignored readonly
//!
//! # Sending checks. These cost credit and deliver real messages.
//! export KAVENEGAR_TEST_RECEPTOR=09177788155
//! export KAVENEGAR_TEST_SENDER=10004346          # optional
//! export KAVENEGAR_TEST_TEMPLATE=verify-login    # optional, for the OTP test
//! cargo test --test live -- --ignored sending --test-threads=1
//! ```
//!
//! A test whose environment variable is missing skips itself with a printed
//! note rather than failing, so a partial setup still tells you something.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kavenegar::{ApiStatus, Error, Kavenegar};

/// Builds a client, or returns `None` so the caller can skip.
fn client() -> Option<Kavenegar> {
    match std::env::var("KAVENEGAR_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(Kavenegar::new(key)),
        _ => {
            eprintln!("skipped: set KAVENEGAR_API_KEY to run this test");
            None
        }
    }
}

/// Reports an account-level block clearly instead of as a crate failure.
///
/// A key can be valid and still be refused: an account that has not finished
/// identity verification is rejected on every endpoint.
fn is_account_blocked(err: &Error) -> bool {
    matches!(
        err.api_status(),
        Some(ApiStatus::AccountNotVerified) | Some(ApiStatus::AccountInactive)
    )
}

macro_rules! skip_if_blocked {
    ($err:expr) => {
        if is_account_blocked(&$err) {
            eprintln!("skipped: {} — fix this in the panel, not in the code", $err);
            return;
        }
    };
}

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => {
            eprintln!("skipped: set {name} to run this test");
            None
        }
    }
}

/// Reads the server clock. The cheapest possible proof that the transport,
/// TLS, URL shape and envelope parsing all work.
#[tokio::test]
#[ignore = "talks to the real API"]
async fn readonly_server_time() {
    let Some(client) = client() else { return };

    let now = match client.utils().server_time().await {
        Ok(now) => now,
        Err(e) => {
            skip_if_blocked!(e);
            panic!("getdate failed: {e}");
        }
    };

    assert!(now.unix_time > 1_700_000_000, "implausible clock: {now:?}");
    println!("server time: {} (unix {})", now.datetime, now.unix_time);
}

/// Confirms the API key is accepted and reports the balance.
#[tokio::test]
#[ignore = "talks to the real API"]
async fn readonly_account_info() {
    let Some(client) = client() else { return };

    match client.account().info().await {
        Ok(info) => {
            println!(
                "credit: {} Rials, type: {}, expires: {}",
                info.remaining_credit, info.account_type, info.expire_date
            );
            assert!(!info.account_type.is_empty());
        }
        Err(Error::Api(ref e)) if e.status() == ApiStatus::InvalidApiKey => {
            panic!("KAVENEGAR_API_KEY was rejected: {e}");
        }
        Err(e) => {
            skip_if_blocked!(e);
            panic!("account/info failed: {e}");
        }
    }
}

/// Lists verification templates, exercising the paginated decoding path.
#[tokio::test]
#[ignore = "talks to the real API"]
async fn readonly_template_list() {
    let Some(client) = client() else { return };

    let page = match client.verify().templates().await {
        Ok(page) => page,
        Err(e) => {
            skip_if_blocked!(e);
            panic!("templatelist failed: {e}");
        }
    };

    println!(
        "{} templates (page {} of {})",
        page.total_count, page.current_page, page.total_pages
    );
    for template in &page {
        println!("  {} [{}]", template.name, template.approval_status);
    }
}

/// Reads recent sends. Needs the IP allowlist configured in the panel, so a
/// 407 here is a configuration problem, not a crate bug.
#[tokio::test]
#[ignore = "talks to the real API"]
async fn readonly_latest_outbox() {
    let Some(client) = client() else { return };

    match client.sms().latest_outbox().page_size(5).await {
        Ok(messages) => {
            println!("{} recent messages", messages.len());
            for m in messages {
                println!("  {} -> {} ({})", m.message_id, m.receptor, m.status);
            }
        }
        Err(Error::Api(ref e)) if e.status() == ApiStatus::AccessDenied => {
            eprintln!("skipped: latestoutbox needs an IP allowlist entry ({e})");
        }
        Err(e) => {
            skip_if_blocked!(e);
            panic!("latestoutbox failed: {e}");
        }
    }
}

/// Sends one real SMS and polls its delivery status.
///
/// This spends credit.
#[tokio::test]
#[ignore = "sends a real message and costs credit"]
async fn sending_sms_round_trip() {
    let Some(client) = client() else { return };
    let Some(receptor) = env("KAVENEGAR_TEST_RECEPTOR") else {
        return;
    };

    let mut request = client
        .sms()
        .send([receptor.as_str()], "تست کتابخانه kavenegar برای Rust")
        .tag("crate-live-test");
    if let Some(sender) = std::env::var("KAVENEGAR_TEST_SENDER")
        .ok()
        .filter(|s| !s.is_empty())
    {
        request = request.sender(sender);
    }

    let sent = match request.await {
        Ok(sent) => sent,
        Err(e) => {
            skip_if_blocked!(e);
            panic!("send failed: {e}");
        }
    };
    assert_eq!(sent.len(), 1, "expected one result per recipient");

    let message = &sent[0];
    println!("sent id {} status {}", message.message_id, message.status);
    assert!(message.message_id > 0);

    // Delivery is asynchronous, so give it a few seconds before checking.
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let statuses = client
        .sms()
        .status([message.message_id])
        .await
        .expect("status failed");

    assert_eq!(statuses.len(), 1);
    println!("status after 5s: {}", statuses[0].status);
    assert_eq!(statuses[0].message_id, message.message_id);
}

/// Sends a real one-time password through a template.
///
/// This spends credit and needs `KAVENEGAR_TEST_TEMPLATE` to name a template
/// approved in your panel.
#[tokio::test]
#[ignore = "sends a real message and costs credit"]
async fn sending_verify_lookup() {
    let Some(client) = client() else { return };
    let Some(receptor) = env("KAVENEGAR_TEST_RECEPTOR") else {
        return;
    };
    let Some(template) = env("KAVENEGAR_TEST_TEMPLATE") else {
        return;
    };

    match client.verify().lookup(&receptor, &template, "48391").await {
        Ok(sent) => {
            println!("otp sent id {} status {}", sent.message_id, sent.status);
            assert!(sent.message_id > 0);
        }
        Err(Error::Api(ref e)) if e.status() == ApiStatus::TemplateNotFound => {
            panic!("no template named `{template}`: {e}");
        }
        Err(e) => {
            skip_if_blocked!(e);
            panic!("lookup failed: {e}");
        }
    }
}

/// Confirms a bad key is reported as [`ApiStatus::InvalidApiKey`] rather than
/// as a transport error. Costs nothing and needs no configuration.
#[tokio::test]
#[ignore = "talks to the real API"]
async fn readonly_rejects_a_bad_key() {
    let client = Kavenegar::new("definitely-not-a-real-api-key");

    let err = client
        .account()
        .info()
        .await
        .expect_err("a bogus key should not succeed");

    println!("bad key reported as: {err}");
    assert!(
        matches!(err, Error::Api(_)),
        "expected an API error, got {err:?}"
    );
    assert!(!err.is_retryable());
}
