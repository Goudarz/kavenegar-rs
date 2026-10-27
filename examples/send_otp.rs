//! Sends a one-time password through a verification template, then polls until
//! the message reaches the handset.
//!
//! The template must already exist and be approved in your Kavenegar panel.
//!
//! ```text
//! KAVENEGAR_API_KEY=... cargo run --example send_otp -- 09177788155 verify-login
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::time::Duration;

use kavenegar::{ApiStatus, Error, Kavenegar};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key =
        std::env::var("KAVENEGAR_API_KEY").map_err(|_| "set KAVENEGAR_API_KEY to your API key")?;

    let mut args = std::env::args().skip(1);
    let receptor = args.next().ok_or("usage: send_otp <receptor> <template>")?;
    let template = args.next().ok_or("usage: send_otp <receptor> <template>")?;

    // In a real application this belongs in your session store, not in a log.
    let code = otp_code()?;

    let client = Kavenegar::new(api_key);

    let sent = match client.verify().lookup(&receptor, &template, &code).await {
        Ok(sent) => sent,
        Err(Error::Api(e)) if e.status() == ApiStatus::TemplateNotFound => {
            return Err(format!("no template named `{template}` — create it in the panel").into());
        }
        Err(e) => return Err(e.into()),
    };

    println!("sent id {} to {}", sent.message_id, sent.receptor);

    // Delivery is asynchronous, so the status right after sending is "queued".
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_secs(3)).await;

        let Some(current) = client.sms().status([sent.message_id]).await?.pop() else {
            continue;
        };
        println!("status: {}", current.status);

        if current.status.is_delivered() || current.status.is_final_failure() {
            break;
        }
    }

    Ok(())
}

/// A uniformly random 5-digit code from the operating system's secure
/// random source.
///
/// Rejection sampling keeps every code equally likely: `u32::MAX` is not a
/// multiple of 100 000, so the few values at the very top are drawn again.
fn otp_code() -> Result<String, getrandom::Error> {
    const RANGE: u32 = 100_000;
    const LIMIT: u32 = u32::MAX - (u32::MAX % RANGE);
    loop {
        let n = getrandom::u32()?;
        if n < LIMIT {
            return Ok(format!("{:05}", n % RANGE));
        }
    }
}
