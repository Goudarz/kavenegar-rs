//! The per-line blocked-recipient list.
//!
//! Recipients who reply with a stop keyword are blocked on the line they
//! replied to, so every method here takes the line number. Kavenegar will not
//! deliver bulk messages to a blocked recipient, so consult this list before a
//! campaign rather than paying for sends that cannot arrive.

use std::future::IntoFuture;

use serde::{Deserialize, Deserializer, Serialize};

use crate::client::{BoxFuture, Kavenegar, Verb};
use crate::de::{lenient_i64, lenient_opt_string, lenient_string};
use crate::error::{Error, Result};
use crate::page::Page;

const MODULE: &str = "line/blocked";

/// The most recipients Kavenegar accepts in one add or exists call.
const MAX_RECEPTORS: usize = 200;

/// The most recipients Kavenegar accepts in one remove call.
const MAX_REMOVALS: usize = 50;

/// Entry point for the blocked-list endpoints, from [`Kavenegar::line`].
#[derive(Debug, Clone, Copy)]
pub struct LineService<'a> {
    client: &'a Kavenegar,
}

impl<'a> LineService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Lists recipients blocked on a line, 200 per page.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let blocked = client.line().blocked("10004346").page(1).await?;
    /// for entry in blocked {
    ///     println!("{} blocked at {}", entry.number, entry.date);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn blocked(self, line_number: impl Into<String>) -> BlockedListBuilder<'a> {
        BlockedListBuilder {
            client: self.client,
            line_number: line_number.into(),
            page_number: None,
            start_date: None,
            block_reason: None,
        }
    }

    /// Adds up to 200 recipients to a line's blocked list.
    ///
    /// The result reports, per recipient, whether it was newly blocked
    /// ([`BlockedState::Active`]) or already present
    /// ([`BlockedState::AlreadyExists`]).
    pub async fn block(
        self,
        line_number: impl Into<String>,
        receptors: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Vec<BlockedResult>> {
        let params = block_params(line_number, receptors, MAX_RECEPTORS)?;
        self.client
            .call_list(Verb::Post, MODULE, "add", params)
            .await
    }

    /// Removes up to 50 recipients from a line's blocked list.
    ///
    /// Only blocks added through the panel or the API can be removed; a block
    /// the recipient placed themselves (by cancelling with `11`) or one placed
    /// by the operator cannot.
    pub async fn unblock(
        self,
        line_number: impl Into<String>,
        receptors: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Unblocked> {
        let params = block_params(line_number, receptors, MAX_REMOVALS)?;
        // Documented as an HTTP DELETE with its parameters in the query.
        self.client
            .call_one(Verb::Delete, MODULE, "remove", params)
            .await
    }

    /// Checks whether up to 200 recipients are on a line's blocked list.
    ///
    /// Each result carries [`BlockedState::Active`] when the recipient is
    /// blocked, or [`BlockedState::NotExists`] when it is not.
    pub async fn is_blocked(
        self,
        line_number: impl Into<String>,
        receptors: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Vec<BlockedResult>> {
        let params = block_params(line_number, receptors, MAX_RECEPTORS)?;
        // Documented as an HTTP GET.
        self.client
            .call_list(Verb::Get, MODULE, "exists", params)
            .await
    }
}

/// Builds a call to `line/blocked/list`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct BlockedListBuilder<'a> {
    client: &'a Kavenegar,
    line_number: String,
    page_number: Option<u32>,
    start_date: Option<i64>,
    block_reason: Option<BlockReason>,
}

impl<'a> BlockedListBuilder<'a> {
    /// Selects a page, counting from 1.
    ///
    /// A page holds 200 records; [`Page::next_page`] says whether another
    /// follows.
    pub fn page(mut self, page_number: u32) -> Self {
        self.page_number = Some(page_number);
        self
    }

    /// Lists blocks created at or after this Unix timestamp.
    ///
    /// Without it, Kavenegar lists only today's blocks.
    pub fn start_date(mut self, start_date: i64) -> Self {
        self.start_date = Some(start_date);
        self
    }

    /// Lists only blocks created for one reason. Defaults to all reasons.
    pub fn block_reason(mut self, block_reason: BlockReason) -> Self {
        self.block_reason = Some(block_reason);
        self
    }
}

impl<'a> IntoFuture for BlockedListBuilder<'a> {
    type Output = Result<Page<BlockedNumber>>;
    type IntoFuture = BoxFuture<'a, Result<Page<BlockedNumber>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            if self.line_number.trim().is_empty() {
                return Err(Error::invalid("line_number", "must not be empty"));
            }
            let mut params = vec![("linenumber", self.line_number)];
            if let Some(page_number) = self.page_number {
                params.push(("pagenumber", page_number.to_string()));
            }
            if let Some(start_date) = self.start_date {
                params.push(("startdate", start_date.to_string()));
            }
            if let Some(block_reason) = self.block_reason {
                params.push(("blockreason", block_reason.code().to_string()));
            }
            self.client
                .call_page(Verb::Post, MODULE, "list", params)
                .await
        })
    }
}

/// An entry in a line's blocked list, as returned by `line/blocked/list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct BlockedNumber {
    /// The blocked recipient number, partially masked by the API.
    #[serde(default, deserialize_with = "lenient_string")]
    pub number: String,
    /// Why the recipient was blocked.
    #[serde(rename = "blockreason", default)]
    pub block_reason: BlockReason,
    /// When the block was recorded, as a Unix timestamp.
    #[serde(default, deserialize_with = "lenient_i64")]
    pub date: i64,
}

/// Why a recipient is on a line's blocked list.
///
/// Unknown codes are preserved in [`BlockReason::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum BlockReason {
    /// Added through the API (0).
    WebService,
    /// Added in the Kavenegar panel (1).
    Panel,
    /// The recipient opted out by sending `11` (2).
    OptOut,
    /// Added by a Kavenegar administrator (3).
    Admin,
    /// Recorded without a reason (10).
    #[default]
    Unknown,
    /// A code this crate does not recognise.
    Other(i64),
}

impl BlockReason {
    /// The numeric code as sent by the API.
    pub fn code(self) -> i64 {
        match self {
            BlockReason::WebService => 0,
            BlockReason::Panel => 1,
            BlockReason::OptOut => 2,
            BlockReason::Admin => 3,
            BlockReason::Unknown => 10,
            BlockReason::Other(c) => c,
        }
    }

    /// Whether [`LineService::unblock`] can lift this block. Only blocks
    /// added through the panel or the API can be removed.
    pub fn is_removable(self) -> bool {
        matches!(self, BlockReason::WebService | BlockReason::Panel)
    }
}

impl From<i64> for BlockReason {
    fn from(code: i64) -> Self {
        match code {
            0 => BlockReason::WebService,
            1 => BlockReason::Panel,
            2 => BlockReason::OptOut,
            3 => BlockReason::Admin,
            10 => BlockReason::Unknown,
            other => BlockReason::Other(other),
        }
    }
}

impl<'de> Deserialize<'de> for BlockReason {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // Documented as a string, shown as a number in the example.
        Ok(BlockReason::from(lenient_i64(d)?))
    }
}

impl Serialize for BlockReason {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_i64(self.code())
    }
}

/// The outcome for one recipient of a block or existence check.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct BlockedResult {
    /// The line the block applies to.
    #[serde(rename = "linenumber", default, deserialize_with = "lenient_string")]
    pub line_number: String,
    /// The recipient this outcome refers to.
    #[serde(default, deserialize_with = "lenient_string")]
    pub receptor: String,
    /// What happened, or what was found.
    #[serde(default)]
    pub status: BlockedState,
}

/// The outcome of [`LineService::unblock`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Unblocked {
    /// Kavenegar's summary, such as `Done-removed 1 item from blacklist`.
    #[serde(default, deserialize_with = "lenient_string")]
    pub message: String,
}

/// The per-recipient status reported by the blocked-list endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[non_exhaustive]
pub enum BlockedState {
    /// The recipient is on the blocked list — newly added, or found by a check.
    Active,
    /// The recipient was already blocked, so nothing changed.
    AlreadyExists,
    /// The recipient is not on the blocked list.
    NotExists,
    /// A state this crate does not recognise.
    #[default]
    Other,
}

impl BlockedState {
    /// Whether the recipient is on the blocked list.
    pub fn is_blocked(&self) -> bool {
        matches!(self, BlockedState::Active | BlockedState::AlreadyExists)
    }
}

impl<'de> Deserialize<'de> for BlockedState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = lenient_opt_string(d)?.unwrap_or_default();
        Ok(match raw.to_ascii_lowercase().as_str() {
            "active" => BlockedState::Active,
            "alreadyexists" => BlockedState::AlreadyExists,
            "notexists" => BlockedState::NotExists,
            _ => BlockedState::Other,
        })
    }
}

/// Validates and encodes the parameters shared by add, remove and exists.
fn block_params(
    line_number: impl Into<String>,
    receptors: impl IntoIterator<Item = impl Into<String>>,
    max: usize,
) -> Result<Vec<(&'static str, String)>> {
    let line_number = line_number.into();
    if line_number.trim().is_empty() {
        return Err(Error::invalid("line_number", "must not be empty"));
    }

    let receptors: Vec<String> = receptors.into_iter().map(Into::into).collect();
    if receptors.is_empty() {
        return Err(Error::invalid(
            "receptors",
            "at least one recipient is required",
        ));
    }
    if receptors.len() > max {
        return Err(Error::invalid(
            "receptors",
            format!("at most {max} recipients per call, got {}", receptors.len()),
        ));
    }

    Ok(vec![
        ("linenumber", line_number),
        ("receptor", receptors.join(",")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_requires_a_line_and_a_recipient() {
        let err = block_params("", ["0912"], MAX_RECEPTORS).unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "line_number",
                ..
            }
        ));

        let err = block_params("10004346", Vec::<String>::new(), MAX_RECEPTORS).unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "receptors",
                ..
            }
        ));
    }

    #[test]
    fn block_and_exists_cap_at_two_hundred_and_remove_at_fifty() {
        let numbers =
            |n: usize| -> Vec<String> { (0..n).map(|i| format!("0912000{i:04}")).collect() };

        assert!(block_params("10004346", numbers(200), MAX_RECEPTORS).is_ok());
        assert!(block_params("10004346", numbers(201), MAX_RECEPTORS).is_err());
        assert!(block_params("10004346", numbers(50), MAX_REMOVALS).is_ok());
        assert!(block_params("10004346", numbers(51), MAX_REMOVALS).is_err());
    }

    #[test]
    fn block_joins_recipients_with_commas() {
        let params = block_params("10004346", ["0912", "0913"], MAX_RECEPTORS).unwrap();
        assert_eq!(params[0], ("linenumber", "10004346".to_owned()));
        assert_eq!(params[1], ("receptor", "0912,0913".to_owned()));
    }

    #[test]
    fn blocked_state_parses_the_documented_values() {
        let json = r#"[
            {"linenumber":"12345","receptor":"0911","status":"Active"},
            {"linenumber":"12345","receptor":"0922","status":"AlreadyExists"},
            {"linenumber":"12345","receptor":"0933","status":"NotExists"},
            {"linenumber":"12345","receptor":"0944","status":"Surprise"},
            {"linenumber":12345,"receptor":"0955","status":null}
        ]"#;
        let results: Vec<BlockedResult> = serde_json::from_str(json).unwrap();

        assert_eq!(results[0].status, BlockedState::Active);
        assert!(results[0].status.is_blocked());
        assert_eq!(results[1].status, BlockedState::AlreadyExists);
        assert!(results[1].status.is_blocked());
        assert_eq!(results[2].status, BlockedState::NotExists);
        assert!(!results[2].status.is_blocked());
        assert_eq!(results[3].status, BlockedState::Other);
        assert_eq!(results[4].status, BlockedState::Other);
        assert_eq!(results[4].line_number, "12345");
    }

    #[test]
    fn blocked_list_entries_use_the_list_shape() {
        // `blockreason` is documented as a string but shown as a number.
        let json = r#"[{"number":"09*********","blockreason":0,"date":1740300377},
                       {"number":"09*********","blockreason":"2","date":0}]"#;
        let entries: Vec<BlockedNumber> = serde_json::from_str(json).unwrap();
        assert_eq!(entries[0].number, "09*********");
        assert_eq!(entries[0].date, 1740300377);
        assert_eq!(entries[0].block_reason, BlockReason::WebService);
        assert!(entries[0].block_reason.is_removable());
        assert_eq!(entries[1].block_reason, BlockReason::OptOut);
        assert!(!entries[1].block_reason.is_removable());
    }
}
