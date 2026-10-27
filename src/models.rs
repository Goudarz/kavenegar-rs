//! Types shared by more than one service.
//!
//! Types used by a single service live in that service's module instead.

#![cfg(any(feature = "sms", feature = "verify", feature = "call"))]

use serde::{Deserialize, Deserializer, Serialize};

use crate::de::{lenient_i64, lenient_opt_string, lenient_string};

/// A single message record, as returned by send, status and inbox methods.
///
/// Not every endpoint fills every field: `status.json` returns only the
/// identifier and status, and the inbox endpoints return no status at all, so
/// missing fields fall back to their defaults rather than failing to decode.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Message {
    /// Kavenegar's identifier for the message.
    #[serde(rename = "messageid", default, deserialize_with = "lenient_i64")]
    pub message_id: i64,

    /// The message body. Empty on endpoints that do not echo it back.
    #[serde(default, deserialize_with = "lenient_string")]
    pub message: String,

    /// Delivery state. [`MessageStatus::Unknown`] on endpoints that report
    /// none, such as the inbox.
    #[serde(default)]
    pub status: MessageStatus,

    /// The Persian description of `status`, as sent by the API.
    #[serde(rename = "statustext", default, deserialize_with = "lenient_string")]
    pub status_text: String,

    /// The line the message was sent from.
    #[serde(default, deserialize_with = "lenient_string")]
    pub sender: String,

    /// The recipient number.
    #[serde(default, deserialize_with = "lenient_string")]
    pub receptor: String,

    /// Send time, as a Unix timestamp in seconds.
    #[serde(default, deserialize_with = "lenient_i64")]
    pub date: i64,

    /// Cost in Rials.
    #[serde(default, deserialize_with = "lenient_i64")]
    pub cost: i64,

    /// Your own identifier, echoed back by the local-id endpoints.
    ///
    /// Kept as text because the API sends it as a string on some endpoints and
    /// a number on others.
    #[serde(
        rename = "localid",
        default,
        deserialize_with = "lenient_opt_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub local_id: Option<String>,
}

/// Delivery state of a message.
///
/// Unknown codes are preserved in [`MessageStatus::Other`] so a new
/// server-side state never turns into a decode failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum MessageStatus {
    /// The response carried no status (0). This is what inbox records hold,
    /// since a received message has no delivery state.
    #[default]
    Unknown,
    /// Queued for sending (1).
    Queued,
    /// Scheduled for a future time (2).
    Scheduled,
    /// Handed to the operator (4).
    SentToCenter,
    /// Sent by the operator (5).
    Sent,
    /// Delivery failed (6).
    Failed,
    /// Delivered to the handset (10).
    Delivered,
    /// Confirmed undelivered (11).
    Undelivered,
    /// Cancelled before sending (13).
    Canceled,
    /// Blocked; the recipient opted out (14).
    Blocked,
    /// An incoming message (50). Not in the current published table.
    Received,
    /// The message id was not valid (100).
    Invalid,
    /// A state this crate does not recognise.
    Other(i64),
}

impl MessageStatus {
    /// The numeric code as sent by the API.
    #[must_use]
    pub fn code(self) -> i64 {
        match self {
            MessageStatus::Unknown => 0,
            MessageStatus::Queued => 1,
            MessageStatus::Scheduled => 2,
            MessageStatus::SentToCenter => 4,
            MessageStatus::Sent => 5,
            MessageStatus::Failed => 6,
            MessageStatus::Delivered => 10,
            MessageStatus::Undelivered => 11,
            MessageStatus::Canceled => 13,
            MessageStatus::Blocked => 14,
            MessageStatus::Received => 50,
            MessageStatus::Invalid => 100,
            MessageStatus::Other(c) => c,
        }
    }

    /// Whether the message reached the handset.
    #[must_use]
    pub fn is_delivered(self) -> bool {
        self == MessageStatus::Delivered
    }

    /// Whether the message failed for good and will not progress further.
    #[must_use]
    pub fn is_final_failure(self) -> bool {
        matches!(
            self,
            MessageStatus::Failed
                | MessageStatus::Undelivered
                | MessageStatus::Canceled
                | MessageStatus::Blocked
                | MessageStatus::Invalid
        )
    }

    /// Whether the message is still moving through the pipeline.
    #[must_use]
    pub fn is_pending(self) -> bool {
        matches!(
            self,
            MessageStatus::Queued
                | MessageStatus::Scheduled
                | MessageStatus::SentToCenter
                | MessageStatus::Sent
        )
    }
}

impl From<i64> for MessageStatus {
    fn from(code: i64) -> Self {
        match code {
            0 => MessageStatus::Unknown,
            1 => MessageStatus::Queued,
            2 => MessageStatus::Scheduled,
            4 => MessageStatus::SentToCenter,
            5 => MessageStatus::Sent,
            6 => MessageStatus::Failed,
            10 => MessageStatus::Delivered,
            11 => MessageStatus::Undelivered,
            13 => MessageStatus::Canceled,
            14 => MessageStatus::Blocked,
            50 => MessageStatus::Received,
            100 => MessageStatus::Invalid,
            other => MessageStatus::Other(other),
        }
    }
}

impl From<MessageStatus> for i64 {
    fn from(status: MessageStatus) -> Self {
        status.code()
    }
}

impl<'de> Deserialize<'de> for MessageStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(MessageStatus::from(lenient_i64(d)?))
    }
}

impl Serialize for MessageStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(self.code())
    }
}

impl std::fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            MessageStatus::Unknown => "unknown",
            MessageStatus::Queued => "queued",
            MessageStatus::Scheduled => "scheduled",
            MessageStatus::SentToCenter => "sent to center",
            MessageStatus::Sent => "sent",
            MessageStatus::Failed => "failed",
            MessageStatus::Delivered => "delivered",
            MessageStatus::Undelivered => "undelivered",
            MessageStatus::Canceled => "canceled",
            MessageStatus::Blocked => "blocked",
            MessageStatus::Received => "received",
            MessageStatus::Invalid => "invalid",
            MessageStatus::Other(_) => "unrecognised",
        };
        write!(f, "{name} ({})", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_decodes_partial_payloads() {
        let json = r#"{"messageid":85463238,"status":10,"statustext":"delivered"}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.message_id, 85463238);
        assert_eq!(m.status, MessageStatus::Delivered);
        assert!(m.status.is_delivered());
        assert_eq!(m.sender, "");
        assert_eq!(m.cost, 0);
    }

    #[test]
    fn unknown_message_status_is_preserved() {
        let m: Message = serde_json::from_str(r#"{"messageid":1,"status":77}"#).unwrap();
        assert_eq!(m.status, MessageStatus::Other(77));
        assert_eq!(m.status.code(), 77);
        assert!(!m.status.is_pending());
        assert!(!m.status.is_final_failure());
    }

    #[test]
    fn inbox_records_do_not_pretend_to_be_queued() {
        let json = r#"{"messageid":35850015,"message":"hi","sender":"0912",
                       "receptor":"3000202030","date":1357206241}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.status, MessageStatus::Unknown);
        assert!(!m.status.is_pending());
        assert!(!m.status.is_final_failure());
    }

    #[test]
    fn loosely_typed_fields_still_decode() {
        // `localid` arrives quoted in the docs but may arrive bare, and
        // optional text may arrive as null.
        let json = r#"{"messageid":"85463238","localid":450,"status":"10",
                       "statustext":null,"sender":null,"cost":"120"}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.message_id, 85463238);
        assert_eq!(m.local_id.as_deref(), Some("450"));
        assert_eq!(m.status, MessageStatus::Delivered);
        assert_eq!(m.status_text, "");
        assert_eq!(m.cost, 120);
    }

    #[test]
    fn status_predicates_partition_states() {
        assert!(MessageStatus::Queued.is_pending());
        assert!(MessageStatus::Blocked.is_final_failure());
        assert!(!MessageStatus::Delivered.is_final_failure());
    }
}
