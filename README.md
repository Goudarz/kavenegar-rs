# kavenegar

[![crates.io](https://img.shields.io/crates/v/kavenegar.svg)](https://crates.io/crates/kavenegar)
[![docs.rs](https://docs.rs/kavenegar/badge.svg)](https://docs.rs/kavenegar)
[![CI](https://github.com/goudarz/kavenegar-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/goudarz/kavenegar-rs/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/kavenegar.svg)](LICENSE)

An async Rust client for the [Kavenegar](https://kavenegar.com) SMS, OTP and
voice-call API.

**[راهنمای فارسی →](README.fa.md)**

- All 32 endpoints from the [REST docs](https://kavenegar.com/rest.html)
- Typed status codes that never break when Kavenegar adds a new one
- Mistakes caught before you spend a request
- No date or TLS baggage — `rustls` by default, nothing from the system

## Contents

- [Install](#install)
- [Get an API key](#get-an-api-key)
- [Quick start](#quick-start)
- [Usage](#usage)
  - [Sending an SMS](#sending-an-sms)
  - [One-time passwords](#one-time-passwords)
  - [Checking delivery](#checking-delivery)
  - [Voice calls](#voice-calls)
  - [Account credit](#account-credit)
  - [Receiving messages](#receiving-messages)
  - [Blocked recipients](#blocked-recipients)
  - [Media files](#media-files)
  - [Templates](#templates)
- [Handling errors](#handling-errors)
- [Pagination](#pagination)
- [All methods](#all-methods)
- [Cargo features](#cargo-features)
- [Testing](#testing)
- [Things worth knowing](#things-worth-knowing)
- [License](#license)

## Install

```bash
cargo add kavenegar
cargo add tokio --features macros,rt-multi-thread
```

Or add it by hand:

```toml
[dependencies]
kavenegar = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Needs Rust 1.88 or newer.

## Get an API key

Sign up at [kavenegar.com](https://kavenegar.com), then copy your key from the
panel under **تنظیمات › کلید وب‌سرویس**.

Keep it out of your source code. Read it from the environment instead:

```bash
export KAVENEGAR_API_KEY=your-key-here
```

## Quick start

```rust
use kavenegar::Kavenegar;

#[tokio::main]
async fn main() -> Result<(), kavenegar::Error> {
    let client = Kavenegar::new(std::env::var("KAVENEGAR_API_KEY").unwrap());

    let sent = client.sms().send(["09121234567"], "Hello!").await?;
    println!("queued as {}", sent[0].message_id);

    Ok(())
}
```

Build the client once and share it. Cloning is cheap and clones reuse the same
connection pool, so put it in your application state rather than making a new
one per request.

## Usage

Calls with optional parameters return a builder. Set what you need, then
`await` the builder itself — there is no separate `send()` step.

### Sending an SMS

```rust
// Simple.
let sent = client.sms().send(["09121234567"], "Hello!").await?;

// With options.
use kavenegar::MessageType;

let sent = client
    .sms()
    .send(["09121234567", "09131234567"], "Your code: 1234")
    .sender("10004346")          // a specific line
    .message_type(MessageType::Flash)
    .tag("signup")               // groups it in your reports
    .await?;

// A different message per recipient, in one call.
let sent = client
    .sms()
    .send_array(
        ["09121234567", "09131234567"],
        ["First message", "Second message"],
    )
    .await?;
```

### One-time passwords

Use this rather than a plain SMS for sign-in codes. It sends a template you
registered in the panel, needs no dedicated line, and still reaches people who
have opted out of bulk messaging.

```rust
let sent = client
    .verify()
    .lookup("09121234567", "verify-login", "48391")
    .await?;
```

Your template holds the wording; you supply only the token:

```text
Your login code is %token
```

A template can take up to five substitutions:

```rust
let sent = client
    .verify()
    .lookup("09121234567", "order-shipped", "48391")
    .token2("Tehran")
    .token10("Ali Rezaei")     // token10 and token20 allow spaces
    .await?;
```

To read it out over a phone call instead:

```rust
use kavenegar::VerifyChannel;

let sent = client
    .verify()
    .lookup("09121234567", "verify-login", "48391")
    .channel(VerifyChannel::Call)
    .await?;
```

### Checking delivery

Sending is asynchronous, so the status right after sending is "queued". Check
back with the message id:

```rust
let statuses = client.sms().status([sent.message_id]).await?;

for m in statuses {
    println!("{}: {}", m.receptor, m.status);

    if m.status.is_delivered() {
        println!("  arrived");
    } else if m.status.is_final_failure() {
        println!("  failed for good, stop polling");
    } else if m.status.is_pending() {
        println!("  still on its way");
    }
}
```

### Voice calls

```rust
let calls = client
    .call()
    .text_to_speech(["09121234567"], "Your order has shipped")
    .repeat(2)
    .await?;
```

### Account credit

```rust
let info = client.account().info().await?;
println!("{} Rials remaining", info.remaining_credit);
```

### Receiving messages

```rust
// Reads unread messages and marks them read. Each message comes back once.
let received = client.sms().receive("3000202030").await?;

// Reads without consuming, so a crash mid-batch loses nothing.
let page = client.sms().inbox_paged("3000202030").unread().await?;
```

### Blocked recipients

People who reply with a stop keyword are blocked on the line they replied to,
so every method here takes the line number first.

```rust
let page = client.line().blocked("10004346").await?;

let results = client.line().block("10004346", ["09121234567"]).await?;
let results = client.line().unblock("10004346", ["09121234567"]).await?;

let results = client.line().is_blocked("10004346", ["09121234567"]).await?;
if results[0].status.is_blocked() {
    println!("do not send to this number");
}
```

### Media files

For messenger lines. Images up to 10 MB, MP4 video up to 20 MB and 60 seconds.

```rust
let bytes = tokio::fs::read("photo.jpg").await?;
let media = client.media().upload("photo.jpg", bytes).await?;
println!("uploaded as {}", media.id);

let page = client.media().list().await?;
let one = client.media().get(&media.id).await?;
client.media().delete(&media.id).await?;
```

### Templates

Manage verification templates from code instead of the panel.

```rust
use kavenegar::{SendMethod, SourceType};

let template = client
    .verify()
    .add_template("LoginOtp", SourceType::Website, SendMethod::Sms)
    .text_message("Your login code is %token")
    .await?;

let page = client.verify().templates().await?;
let one = client.verify().template(template.id).await?;
let copy = client.verify().clone_template("LoginOtpV2").from_id(template.id).await?;
client.verify().delete_template(template.id).await?;
```

Resellers can act on a customer's templates by naming the sub-account:

```rust
let page = client
    .verify()
    .templates()
    .sub_account_api_key("the-customers-key")
    .await?;
```

## Handling errors

Kavenegar reports most problems with HTTP 200 and a status code in the body, so
match on `Error::Api` rather than the HTTP status.

```rust
use kavenegar::{ApiStatus, Error};

match client.sms().send(["09121234567"], "Hello").await {
    Ok(sent) => println!("queued as {}", sent[0].message_id),

    Err(Error::Api(e)) if e.status() == ApiStatus::InsufficientCredit => {
        eprintln!("out of credit: {}", e.message());
    }
    Err(Error::Api(e)) if e.status() == ApiStatus::InvalidReceptor => {
        eprintln!("bad phone number");
    }
    Err(e) if e.is_retryable() => {
        eprintln!("temporary problem, try again: {e}");
    }
    Err(e) => return Err(e),
}
```

Every documented code has a named variant. Codes Kavenegar adds later arrive as
`ApiStatus::Other(i32)` instead of failing to parse, so your build keeps
working.

`is_retryable()` is true only for genuinely transient failures — a busy server,
a rate limit, a network timeout. It is never true for a bad key or an empty
balance, where retrying would just repeat the failure.

Some mistakes are caught before a request goes out at all, so they cost you
nothing:

```rust
// Two recipients but one message.
let err = client.sms().send_array(["0912...", "0913..."], ["only one"]).await;
// -> InvalidParameter { parameter: "messages", .. }
```

## Pagination

Listing calls return a `Page<T>`. It behaves like a slice and carries the
counters you need to walk the rest:

```rust
let mut page = client.verify().templates().await?;

loop {
    for template in &page {
        println!("{}", template.name);
    }

    let Some(next) = page.next_page() else { break };
    page = client.verify().templates().page(next as u32).await?;
}
```

## All methods

| Service | Methods |
| --- | --- |
| `sms()` | `send`, `send_array`, `status`, `status_by_local_id`, `status_by_receptor`, `select`, `select_outbox`, `latest_outbox`, `count_outbox`, `cancel`, `receive`, `inbox_paged`, `count_inbox` |
| `verify()` | `lookup`, `templates`, `template`, `add_template`, `update_template`, `clone_template`, `delete_template` |
| `call()` | `text_to_speech` |
| `account()` | `info`, `config`, `update_config` |
| `line()` | `blocked`, `block`, `unblock`, `is_blocked` |
| `media()` | `upload`, `list`, `get`, `delete` |
| `utils()` | `server_time` |

## Cargo features

Everything is on by default, so `kavenegar = "0.1"` just works.

To compile only what you use, turn the defaults off and name the services you
want. **Keep a TLS backend in the list**, or nothing will connect:

```toml
kavenegar = { version = "0.1", default-features = false, features = ["sms", "verify", "rustls-tls"] }
```

| Feature | Default | Enables |
| --- | --- | --- |
| `full` | ✅ | Every service below |
| `sms` | ✅ | `client.sms()` |
| `verify` | ✅ | `client.verify()` |
| `call` | ✅ | `client.call()` |
| `account` | ✅ | `client.account()` |
| `line` | ✅ | `client.line()` |
| `media` | ✅ | `client.media()` — adds `multipart` |
| `utils` | ✅ | `client.utils()` |
| `rustls-tls` | ✅ | TLS through `rustls`, no system OpenSSL |
| `native-tls` | ❌ | TLS through the platform's own stack |

Some ready-made combinations:

```toml
# OTP only — the smallest useful build.
kavenegar = { version = "0.1", default-features = false, features = ["verify", "rustls-tls"] }

# Everything except media, which drops the multipart dependency.
kavenegar = { version = "0.1", default-features = false, features = [
    "sms", "verify", "call", "account", "line", "utils", "rustls-tls",
] }

# Full, on the platform TLS stack instead of rustls.
kavenegar = { version = "0.1", default-features = false, features = ["full", "native-tls"] }
```

To be straight about it: dropping `media` is the only choice that removes a
real dependency. The other flags mainly trim compile time. If you have no
particular reason to trim, keep the default.

## Testing

**Offline tests** need no key and no network. They run a stub HTTP server
in-process:

```bash
cargo test
```

**Live tests** talk to the real API and are `#[ignore]`d so they never run by
accident. The read-only ones cost nothing — start there:

```bash
export KAVENEGAR_API_KEY=your-key
cargo test --test live -- --ignored readonly --nocapture
```

That checks the server clock, your credit, your templates, and that a bad key
is reported properly.

The sending tests deliver real messages and **spend credit**:

```bash
export KAVENEGAR_TEST_RECEPTOR=09121234567
export KAVENEGAR_TEST_SENDER=10004346        # optional
export KAVENEGAR_TEST_TEMPLATE=verify-login  # optional
cargo test --test live -- --ignored sending --nocapture --test-threads=1
```

A test whose variable is missing skips itself with a note rather than failing.

**The examples** are the quickest manual check:

```bash
cargo run --example send_sms -- 09121234567 "Hello"
cargo run --example send_otp -- 09121234567 verify-login
```

**Everything CI runs**, if you want it locally:

```bash
cargo fmt --all --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

## Things worth knowing

- **Timestamps** are Unix seconds, exactly as the API sends them. Convert with
  `chrono` or `jiff` if you want calendar types; this crate adds no date
  dependency. `client.utils().server_time()` reads Kavenegar's own clock.
- **Costs** are in Rials.
- **`sms().receive()` consumes messages** — a second call will not return the
  same ones. Use `sms().inbox_paged()` if you would rather read without
  consuming.
- **`select`, `select_outbox` and `latest_outbox`** need an IP allowlist entry
  in your panel, or they fail with `ApiStatus::AccessDenied`.
- **Trial accounts** can only send to their own owner's number.
- **Tokens must not contain spaces.** Kavenegar splits on whitespace and would
  silently truncate the message, so this crate rejects it up front. Use
  `token10` or `token20` for values with spaces.

## License

[MIT](LICENSE).

This is an unofficial, community-maintained client and is not affiliated with
Kavenegar.
