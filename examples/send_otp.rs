//! Sends a one-time password through a verification template, then polls until
//! the message reaches the handset.
//!
//! The template must already exist and be approved in your Kavenegar panel.
//!
//! ```text
//! KAVENEGAR_API_KEY=... cargo run --example send_otp -- 09121234567 verify-login
//! ```

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
    let code = format!("{:05}", rand_code());

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

/// A throwaway 5-digit code. Use a real CSPRNG in production.
fn rand_code() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    nanos % 100_000
}
