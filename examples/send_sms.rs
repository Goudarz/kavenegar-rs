//! Sends a single SMS and reports what happened.
//!
//! ```text
//! KAVENEGAR_API_KEY=... cargo run --example send_sms -- 09177788155 "سلام"
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kavenegar::{ApiStatus, Error, Kavenegar};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key =
        std::env::var("KAVENEGAR_API_KEY").map_err(|_| "set KAVENEGAR_API_KEY to your API key")?;

    let mut args = std::env::args().skip(1);
    let receptor = args.next().ok_or("usage: send_sms <receptor> <message>")?;
    let message = args.next().ok_or("usage: send_sms <receptor> <message>")?;

    let client = Kavenegar::new(api_key);

    match client.sms().send([receptor], message).tag("example").await {
        Ok(sent) => {
            for m in sent {
                println!("{} -> id {} ({})", m.receptor, m.message_id, m.status);
            }
        }
        Err(Error::Api(e)) if e.status() == ApiStatus::InsufficientCredit => {
            eprintln!("out of credit: {}", e.message());
        }
        Err(e) if e.is_retryable() => eprintln!("transient failure, try again: {e}"),
        Err(e) => return Err(e.into()),
    }

    Ok(())
}
