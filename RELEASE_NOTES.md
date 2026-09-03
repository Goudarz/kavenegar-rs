# v0.1.0 — first release

An async Rust client for the [Kavenegar](https://kavenegar.com) SMS, OTP and
voice-call API. Covers all 32 endpoints from the
[REST documentation](https://kavenegar.com/rest.html).

## Install

```bash
cargo add kavenegar
```

```toml
[dependencies]
kavenegar = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Requires Rust 1.88+.

## Getting started

```rust
use kavenegar::Kavenegar;

#[tokio::main]
async fn main() -> Result<(), kavenegar::Error> {
    let client = Kavenegar::new(std::env::var("KAVENEGAR_API_KEY").unwrap());

    // Send an SMS.
    let sent = client.sms().send(["09177788155"], "Hello!").await?;

    // Send a one-time password through a template.
    let otp = client.verify().lookup("09177788155", "verify-login", "48391").await?;

    Ok(())
}
```

## What's in it

**Every service**

| Service | Methods |
| --- | --- |
| `sms()` | `send`, `send_array`, `status`, `status_by_local_id`, `status_by_receptor`, `select`, `select_outbox`, `latest_outbox`, `count_outbox`, `cancel`, `receive`, `inbox_paged`, `count_inbox` |
| `verify()` | `lookup`, `templates`, `template`, `add_template`, `update_template`, `clone_template`, `delete_template` |
| `call()` | `text_to_speech` |
| `account()` | `info`, `config`, `update_config` |
| `line()` | `blocked`, `block`, `unblock`, `is_blocked` |
| `media()` | `upload`, `list`, `get`, `delete` |
| `utils()` | `server_time` |

**Builders you just await.** One method covers the simple case and the
complicated one, with no separate send step:

```rust
client.sms().send(["0912..."], "Hi").sender("10004346").tag("signup").await?;
```

**Status codes that don't break.** Every documented code has a named variant,
and anything Kavenegar adds later arrives as `Other(i32)` rather than failing
to parse.

**Errors that tell you what to do.** Kavenegar reports failures with HTTP 200
and a code in the body, so `Error::Api` is what you match on.
`is_retryable()` is true only for genuinely transient problems — never for a
bad key or an empty balance.

**Mistakes caught before you pay for them.** Mismatched recipient and message
lists, tokens containing spaces, invalid template names, and over-long
blocked-list batches are all rejected locally.

**Pagination that walks itself.** Listing calls return a `Page<T>` that
behaves like a slice and carries `next_page()`.

**Optional per-service features.** Everything is on by default; turn off what
you don't use.

## Documentation

- [English guide](README.md)
- [راهنمای فارسی](README.fa.md)
- [API reference on docs.rs](https://docs.rs/kavenegar)

## Notes

This release has been verified against a stub server covering the request
encoding, response envelope and every error path, but it has not yet been
exercised against a production account at scale. If you hit a response this
crate decodes wrongly, please open an issue with the raw body — `Error::Decode`
includes it.

Unofficial and community-maintained; not affiliated with Kavenegar.
