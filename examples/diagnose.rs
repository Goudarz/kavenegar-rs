//! Checks an API key against the live account and explains what it finds.
//!
//! Nothing here sends a message or spends credit — every call is read-only.
//!
//! ```text
//! KAVENEGAR_API_KEY=... cargo run --example diagnose
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kavenegar::{ApiStatus, Error, Kavenegar};

#[tokio::main]
async fn main() {
    let Ok(api_key) = std::env::var("KAVENEGAR_API_KEY") else {
        eprintln!("set KAVENEGAR_API_KEY first:");
        eprintln!("  export KAVENEGAR_API_KEY=your-key");
        std::process::exit(1);
    };

    println!(
        "key: {} chars, ending {}",
        api_key.trim().chars().count(),
        tail(&api_key)
    );
    println!("host: {}\n", kavenegar::DEFAULT_BASE_URL);

    let client = Kavenegar::new(api_key);
    let mut blocked = None;

    // 1. The cheapest possible call. If this works, the transport works.
    print!("server clock          ... ");
    match client.utils().server_time().await {
        Ok(t) => println!("ok — {} (unix {})", t.datetime, t.unix_time),
        Err(e) => {
            println!("{}", describe(&e));
            blocked = e.api_status();
        }
    }

    // 2. Credit and account tier.
    print!("account info          ... ");
    match client.account().info().await {
        Ok(i) => println!(
            "ok — {} Rials, type `{}`, expires {}",
            i.remaining_credit, i.account_type, i.expire_date
        ),
        Err(e) => {
            println!("{}", describe(&e));
            blocked = blocked.or(e.api_status());
        }
    }

    // 3. Verification templates, which OTP sending depends on.
    print!("verify templates      ... ");
    match client.verify().templates().await {
        Ok(page) if page.is_empty() => println!("ok — none registered yet"),
        Ok(page) => {
            println!("ok — {} registered", page.total_count);
            for t in &page {
                println!("      · {} [{}]", t.name, t.approval_status);
            }
        }
        Err(e) => println!("{}", describe(&e)),
    }

    // 4. Recent sends. Needs an IP allowlist entry, so failure here is common
    //    and does not mean the key is wrong.
    print!("recent outbox         ... ");
    match client.sms().latest_outbox().page_size(3).await {
        Ok(m) if m.is_empty() => println!("ok — nothing sent yet"),
        Ok(m) => {
            println!("ok — {} recent", m.len());
            for msg in m {
                println!(
                    "      · {} -> {} ({})",
                    msg.message_id, msg.receptor, msg.status
                );
            }
        }
        Err(e) => println!("{}", describe(&e)),
    }

    // 5. Media, which only messenger lines can use.
    print!("media library         ... ");
    match client.media().list().await {
        Ok(page) => println!("ok — {} file(s)", page.total_count),
        Err(e) => println!("{}", describe(&e)),
    }

    println!();
    match blocked {
        Some(ApiStatus::AccountNotVerified) => {
            println!("The key is reaching Kavenegar, but the account has not finished");
            println!("identity verification, so every endpoint refuses it.");
            println!("Fix: complete احراز هویت in the panel, then run this again.");
        }
        Some(ApiStatus::InvalidApiKey) => {
            println!("Kavenegar rejected this key. Copy it again from");
            println!("the panel under Settings > Security Settings.");
        }
        Some(ApiStatus::AccountInactive) => {
            println!("The account is disabled. Contact Kavenegar support.");
        }
        Some(_) | None => println!("Done."),
    }
}

/// Turns an error into one line a human can act on.
fn describe(err: &Error) -> String {
    match err {
        Error::Api(e) => match e.status() {
            ApiStatus::AccessDenied => {
                format!("skipped — needs an IP allowlist entry ({})", e.code())
            }
            ApiStatus::ServicePlanRequired => {
                format!("skipped — needs the advanced plan ({})", e.code())
            }
            status => format!("failed — {} ({})", status.describe(), e.code()),
        },
        Error::Http(_) => format!("failed — could not reach the server: {err}"),
        other => format!("failed — {other}"),
    }
}

/// Shows just enough of the key to tell two apart, without printing it.
fn tail(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    let last: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("...{last}")
}
