# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

While the version stays below `1.0.0`, a minor bump (`0.1` → `0.2`) may contain
breaking changes; patch bumps never do.

## [Unreleased]

## [0.1.0] - 2026-09-03

Initial release. Covers all 32 endpoints documented at
<https://kavenegar.com/rest.html>.

### Added

- `Kavenegar` async client, cheap to clone and sharing one connection pool.
- `sms()` — `send`, `send_array`, `status`, `status_by_local_id`,
  `status_by_receptor`, `select`, `select_outbox`, `latest_outbox`,
  `count_outbox`, `cancel`, `receive`, `inbox_paged`, `count_inbox`.
- `verify()` — `lookup` for one-time passwords, plus full template management:
  `templates`, `template`, `add_template`, `update_template`, `clone_template`,
  `delete_template`.
- `call()` — `text_to_speech` for voice delivery.
- `account()` — `info`, `config`, `update_config`.
- `line()` — `blocked`, `block`, `unblock`, `is_blocked`, all scoped to a line
  as the API requires.
- `media()` — `upload`, `list`, `get`, `delete`.
- `utils()` — `server_time`.
- `Page<T>` for paginated endpoints: dereferences to a slice and carries
  `total_count`, `current_page`, `total_pages`, `page_size` and `next_page()`.
- Typed `ApiStatus`, `MessageStatus`, `MediaStatus` and `BlockedState` enums
  naming every documented code, each with an `Other(_)` variant so a new
  server-side code decodes instead of failing. `ApiStatus` also names `430`
  (account not verified), which the live API returns but the published table
  does not list.
- `Error::is_retryable()` to separate transient failures from permanent ones.
- Local validation before a request is spent: recipient/message/local-id list
  lengths, tokens containing whitespace, template names, `switch_ttl` range,
  `%token` presence, and the 200-recipient blocked-list cap.
- Sub-account support (`sub_account_api_key`, `sub_account_local_id`) on every
  `verify` template endpoint, for resellers managing a customer's templates.
- Hardening against the API key escaping through a log: transport errors are
  re-wrapped as `TransportError` so the request URL — which contains the key —
  never reaches `Display`, `Debug` or the error source chain. Response bodies
  kept for diagnostics are scrubbed of the key too.
- `base_url` refuses plain HTTP to non-loopback hosts, redirects are not
  followed, and response bodies are capped at `MAX_RESPONSE_BYTES` (8 MiB)
  while streaming.
- `SECURITY.md` with the threat model and reporting address; CI runs
  `cargo audit` and `cargo deny` on every push and weekly.
- Per-service Cargo features (`sms`, `verify`, `call`, `account`, `line`,
  `media`, `utils`, and `full`), plus `rustls-tls` (default) and `native-tls`.

[Unreleased]: https://github.com/goudarz/kavenegar-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/goudarz/kavenegar-rs/releases/tag/v0.1.0
