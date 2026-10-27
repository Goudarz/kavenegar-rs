# kavenegar

[![crates.io](https://img.shields.io/crates/v/kavenegar.svg)](https://crates.io/crates/kavenegar)
[![docs.rs](https://docs.rs/kavenegar/badge.svg)](https://docs.rs/kavenegar)
[![CI](https://github.com/goudarz/kavenegar-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/goudarz/kavenegar-rs/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/kavenegar.svg)](LICENSE)

An async Rust client for the [Kavenegar](https://iff.gg/4GQhL9) SMS, OTP and
voice-call API.

**[راهنمای فارسی →](README.fa.md)**

> [!WARNING]
> **This crate is under active development.** The API may change between
> `0.x` releases, and not every endpoint has been exercised against a live
> Kavenegar account yet. If a response fails to decode, please
> [open an issue](https://github.com/goudarz/kavenegar-rs/issues) with the raw
> body — `Error::Decode` includes it.

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
  - [Voice calls (TTS)](#voice-calls-tts)
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
- [Security](#security)
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

Sign up at [kavenegar.com](https://iff.gg/4GQhL9). You can then find your
account's **API KEY** in the panel under **Settings › Security Settings**.

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

    let sent = client.sms().send(["09177788155"], "Hello!").await?;
    println!("queued as {}", sent[0].message_id);

    Ok(())
}
```

Build the client once and use it everywhere. Copy it wherever you like — the
copies share one connection pool, so keep it in your application state rather
than making a new one per request.

## Usage

Calls with optional parameters return a builder. Set what you need, then
`await` the builder itself — there is no separate `send()` step.

### Sending an SMS

```rust
// Simple.
let sent = client.sms().send(["09177788155"], "Hello!").await?;

// With options.
use kavenegar::MessageType;

let sent = client
    .sms()
    .send(["09177788155", "09111234567"], "Your code: 1234")
    .sender("10004346")          // a specific line
    .message_type(MessageType::Flash)
    .tag("signup")               // groups it in your reports
    .await?;

// Media on a messenger line, or a sending policy instead of one line.
let sent = client
    .sms()
    .send(["09177788155"], "Your invoice")
    .media_id(&media.id)         // from client.media().upload(..)
    .policy("night-flow")        // a sending policy defined in your panel
    .await?;

// A different message per recipient, in one call.
let sent = client
    .sms()
    .send_array(
        ["09177788155", "09131234567"],
        ["First message", "Second message"],
    )
    .sender("10004346")          // required here; or .senders([...]) per message
    .await?;
```

### One-time passwords

Use this rather than a plain SMS for sign-in codes. It sends a template you
registered in the panel, needs no dedicated line, and still reaches people who
have opted out of bulk messaging.

```rust
let sent = client
    .verify()
    .lookup("09177788155", "verify-login", "48391")
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
    .lookup("09177788155", "order-shipped", "48391")
    .token2("Shiraz")
    .token10("Goudarz Jafari")     // token10 allows 5 spaces, token20 allows 8
    .await?;
```

To read it out over a phone call instead:

```rust
use kavenegar::VerifyChannel;

let sent = client
    .verify()
    .lookup("09177788155", "verify-login", "48391")
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

### Voice calls (TTS)

```rust
let calls = client
    .call()
    .text_to_speech(["09177788155"], "Your order has shipped")
    .await?;
```

### Account credit

```rust
let info = client.account().info().await?;
println!("{} Rials remaining", info.remaining_credit);
```

### Receiving messages

```rust
// Reads up to 100 unread messages and marks them read, so each comes back once.
let received = client.sms().receive("3000202030").await?;

// The same, 200 per page, with a date window and page counters.
// This marks what it returns as read too.
let page = client.sms().inbox_paged("3000202030").unread().await?;
```

### Blocked recipients

People who reply with a stop keyword are blocked on the line they replied to,
so every method here takes the line number first.

```rust
let page = client.line().blocked("10004346").await?;

let results = client.line().block("10004346", ["09177788155"]).await?;   // up to 200
let done = client.line().unblock("10004346", ["09177788155"]).await?;    // up to 50
println!("{}", done.message);

let results = client.line().is_blocked("10004346", ["09177788155"]).await?;
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

match client.sms().send(["09177788155"], "Hello").await {
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
let err = client
    .sms()
    .send_array(["09121234567", "09131234567"], ["only one"])
    .sender("10004346")
    .await;
// -> InvalidParameter { parameter: "messages", .. }

// A "phone number" that is really two, typed into your sign-up form.
let err = client.verify().lookup("09121234567,09131234567", "verify-login", "4839").await;
// -> InvalidParameter { parameter: "receptor", .. }
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
    page = client.verify().templates().page(next).await?;
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

**Check your account** before anything else. This is read-only and free:

```bash
export KAVENEGAR_API_KEY=your-key
cargo run --example diagnose
```

It probes the clock, credit, templates, outbox and media library, then tells
you in plain terms what works and what to fix:

```text
server clock          ... ok — 03/09/2026 11:42:17 ق.ظ (unix 1788500537)
account info          ... ok — 250000 Rials, type `master`, expires 1798500537
verify templates      ... ok — 2 registered
recent outbox         ... skipped — needs an IP allowlist entry (407)
media library         ... ok — 0 file(s)
```

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
export KAVENEGAR_TEST_RECEPTOR=09177788155
export KAVENEGAR_TEST_SENDER=10004346        # optional
export KAVENEGAR_TEST_TEMPLATE=verify-login  # optional
cargo test --test live -- --ignored sending --nocapture --test-threads=1
```

A test whose variable is missing skips itself with a note rather than failing.

**The examples** are the quickest manual check:

```bash
cargo run --example send_sms -- 09177788155 "Hello"
cargo run --example send_otp -- 09177788155 verify-login
```

**Everything CI runs**, if you want it locally:

```bash
cargo fmt --all --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

## Security

The API key is the asset here, and Kavenegar puts it **in the URL path** rather
than a header. A URL travels further than a header does, so this crate:

- **Keeps the URL out of error messages.** `reqwest` embeds the request URL in
  its own errors; passing those through would write your key into every log
  line that formats an error. Transport failures are re-wrapped to keep the
  diagnosis and drop the URL.
- **Redacts the key** from `Debug` output and from any response body kept for
  diagnostics.
- **Requires TLS 1.2+**, on both TLS backends.
- **Sanitises Kavenegar's error text** before it reaches `Display`, so a
  newline or terminal escape in it cannot forge a log line.
- **Refuses plain HTTP** to non-loopback hosts, so a misconfigured `base_url`
  cannot put the key on the wire in clear text.
- **Never follows redirects**, which would hand the path — and the key in it —
  to a host the server chooses. If you pass your own client through
  `ClientBuilder::http_client`, that one is used as-is: build it with
  `.redirect(reqwest::redirect::Policy::none())`.
- **Caps response bodies at 8 MiB while streaming**, so a reply with no
  declared length cannot exhaust memory.
- **Refuses a phone number that is really a list.** Kavenegar separates
  recipients with commas, so an unchecked `"0912…,0913…"` from a sign-up form
  would turn one OTP into many paid sends (SMS pumping). Every number must be
  plain digits.
- **Keeps secrets out of `Debug`**: the API key, a sub-account's key, and the
  token values of a lookup (your one-time passwords).
- **Validates the API key's characters**, so a stray newline or `/` cannot
  silently send the request to a different URL.

Each of these is covered by a test in [`tests/security.rs`](tests/security.rs).

One thing to know when logging: `Display` on `Error` is safe, but `Debug`
includes response bodies, which routinely contain recipient phone numbers and
may contain message text. Prefer `error!("{e}")` over `error!("{e:?}")` in
production, or treat those logs as personal data.

The full policy, threat model, and reporting address are in
[SECURITY.md](SECURITY.md).

## Things worth knowing

- **Timestamps** are Unix seconds, exactly as the API sends them. Convert with
  `chrono`, `jiff`, or anything else if you want calendar types; this crate
  adds no date dependency. `client.utils().server_time()` reads Kavenegar's own clock.
- **Costs** are in Rials.
- **Reading the inbox marks messages read.** Both `sms().receive()` and
  `sms().inbox_paged()` mark what they return as read, so asking for unread
  messages twice will not return the same ones. Store them before you process
  them.
- **`select`, `select_outbox` and `latest_outbox`** need an IP allowlist entry
  in your panel, or they fail with `ApiStatus::AccessDenied`.
- **Trial accounts** can only send to their own owner's number.
- **A new account must finish identity verification** (احراز هویت) in the
  panel. Until it does, every endpoint returns `ApiStatus::AccountNotVerified`,
  including free ones like `server_time`.
- **Lookup tokens have rules.** Each is at most 100 characters and may not
  contain a newline or an underscore. `token`, `token2` and `token3` may not
  contain spaces; `token10` allows 5 and `token20` allows 8. Kavenegar answers
  a broken rule with error 431, so this crate rejects it before sending.
- **Every phone and line number is checked.** Only ASCII digits with an
  optional leading `+` are accepted (`09121234567`, `+989121234567`,
  `00989121234567`). Kavenegar takes lists as comma-separated text, so without
  this a number typed by an end user, like `"0912…,0913…"`, would quietly
  become several recipients billed to you. Convert Persian digits (`۰۹۱۲`)
  before passing them in.
- **Retrying a send can send twice.** After a timeout the message may already
  be on its way; `is_retryable()` cannot know. Attach `local_ids` before
  retrying a `send`, so Kavenegar drops the duplicate, and issue a fresh code
  rather than resending an OTP.
- **Local ids are numbers.** Kavenegar documents them as integers, so
  `local_ids` takes `i64`. A repeated local id is not sent twice.

## License

This project is released under the [MIT License](LICENSE).

This is an unofficial, open-source client and has no official affiliation with
Kavenegar.

Maintained by **Goudarz Jafari** — [hi@goudarzjafari.com](mailto:hi@goudarzjafari.com).
Source and issues: <https://github.com/goudarz/kavenegar-rs>.
