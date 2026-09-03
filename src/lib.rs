//! An async Rust client for the [Kavenegar](https://kavenegar.com) SMS, OTP
//! and voice-call API.
//!
//! Every endpoint documented at <https://kavenegar.com/rest.html> is covered.
//!
//! # Getting started
//!
//! Build one [`Kavenegar`] client and share it — clones are cheap and reuse the
//! same connection pool.
//!
//! ```no_run
//! use kavenegar::Kavenegar;
//!
//! # async fn run() -> Result<(), kavenegar::Error> {
//! let client = Kavenegar::new(std::env::var("KAVENEGAR_API_KEY").unwrap());
//!
//! let sent = client.sms().send(["09121234567"], "سلام دنیا").await?;
//! println!("queued as {}", sent[0].message_id);
//! # Ok(())
//! # }
//! ```
//!
//! # Request builders
//!
//! Calls that take optional parameters return a builder. Set what you need and
//! `await` the builder itself — there is no separate `send()` step.
//!
//! ```no_run
//! # use kavenegar::MessageType;
//! # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
//! let sent = client
//!     .sms()
//!     .send(["09121234567"], "کد شما: ۱۲۳۴")
//!     .sender("10004346")
//!     .message_type(MessageType::Flash)
//!     .tag("signup")
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! # One-time passwords
//!
//! Prefer [`verify().lookup()`] over a plain SMS for sign-in codes: it uses a
//! template approved in your panel, needs no dedicated line, and still reaches
//! recipients who have opted out of bulk messaging.
//!
//! ```no_run
//! # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
//! let sent = client.verify().lookup("09121234567", "verify-login", "48391").await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Handling failures
//!
//! Kavenegar reports most problems with HTTP 200 and a status code in the body,
//! so match on [`Error::Api`] rather than on the HTTP status.
//!
//! ```no_run
//! # use kavenegar::{ApiStatus, Error};
//! # async fn run(client: kavenegar::Kavenegar) -> Result<(), Error> {
//! match client.sms().send(["09121234567"], "سلام").await {
//!     Ok(sent) => println!("queued as {}", sent[0].message_id),
//!     Err(Error::Api(e)) if e.status() == ApiStatus::InsufficientCredit => {
//!         eprintln!("top up the account: {}", e.message());
//!     }
//!     Err(e) if e.is_retryable() => eprintln!("transient failure, try again: {e}"),
//!     Err(e) => return Err(e),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Cargo features
//!
//! Everything is on by default. To compile only what you call, opt out of the
//! defaults and name the services you want — remembering to keep a TLS backend:
//!
//! ```toml
//! kavenegar = { version = "0.1", default-features = false, features = ["sms", "verify", "rustls-tls"] }
//! ```
//!
//! | Feature | Default | What it enables |
//! | --- | --- | --- |
//! | `full` | yes | Every service below. |
//! | `sms` | yes | [`Kavenegar::sms`] — sending, status, history, inbox. |
//! | `verify` | yes | [`Kavenegar::verify`] — one-time passwords and templates. |
//! | `call` | yes | [`Kavenegar::call`] — text-to-speech voice calls. |
//! | `account` | yes | [`Kavenegar::account`] — credit, expiry, settings. |
//! | `line` | yes | [`Kavenegar::line`] — the blocked-recipient list. |
//! | `media` | yes | [`Kavenegar::media`] — file upload. Adds `multipart`. |
//! | `utils` | yes | [`Kavenegar::utils`] — server time. |
//! | `rustls-tls` | yes | TLS through `rustls`; no system OpenSSL needed. |
//! | `native-tls` | no | TLS through the platform's own stack. |
//!
//! Dropping `media` is the only choice that removes a real dependency
//! (`multipart`); the rest mainly trim compile time.
//!
//! [`verify().lookup()`]: VerifyService::lookup

#![doc(html_root_url = "https://docs.rs/kavenegar/0.1.0")]
#![warn(missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod client;
pub mod error;
pub mod models;
pub mod page;

#[cfg(feature = "account")]
pub mod account;
#[cfg(feature = "call")]
pub mod call;
#[cfg(feature = "line")]
pub mod line;
#[cfg(feature = "media")]
pub mod media;
#[cfg(feature = "sms")]
pub mod sms;
#[cfg(feature = "utils")]
pub mod utils;
#[cfg(feature = "verify")]
pub mod verify;

#[doc(inline)]
pub use crate::client::{ClientBuilder, DEFAULT_BASE_URL, Kavenegar};
#[doc(inline)]
pub use crate::error::{ApiError, ApiStatus, Error, Result};
#[doc(inline)]
pub use crate::page::{Page, PageMeta};

#[cfg(any(feature = "sms", feature = "verify", feature = "call"))]
#[doc(inline)]
pub use crate::models::{Message, MessageStatus};

#[cfg(feature = "account")]
#[doc(inline)]
pub use crate::account::{AccountConfig, AccountInfo, AccountService, Toggle};

#[cfg(feature = "call")]
#[doc(inline)]
pub use crate::call::CallService;

#[cfg(feature = "line")]
#[doc(inline)]
pub use crate::line::{BlockedNumber, BlockedResult, BlockedState, LineService};

#[cfg(feature = "media")]
#[doc(inline)]
pub use crate::media::{Media, MediaDeletion, MediaReview, MediaService, MediaStatus};

#[cfg(feature = "sms")]
#[doc(inline)]
pub use crate::sms::{InboxCount, MessageType, OutboxCount, SmsService};

#[cfg(feature = "utils")]
#[doc(inline)]
pub use crate::utils::{ServerTime, UtilsService};

#[cfg(feature = "verify")]
#[doc(inline)]
pub use crate::verify::{
    FallbackMethod, SendMethod, SourceType, Template, VerifyChannel, VerifyService,
};
