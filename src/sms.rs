//! Sending messages, and querying their status and history.

use std::future::IntoFuture;

use serde::{Deserialize, Serialize};

use crate::check;
use crate::client::{BoxFuture, Kavenegar, Verb};
use crate::de::lenient_i64;
use crate::error::{Error, Result};
use crate::models::Message;
use crate::page::Page;

const MODULE: &str = "sms";

/// The most recipients Kavenegar accepts in one send call.
const MAX_RECIPIENTS: usize = 200;

/// The most message ids Kavenegar accepts in one status, select or cancel call.
const MAX_MESSAGE_IDS: usize = 500;

/// The most records `latestoutbox` returns, and its default.
const MAX_OUTBOX_PAGE: u32 = 500;

/// Entry point for the `sms` endpoints, from [`Kavenegar::sms`].
#[derive(Debug, Clone, Copy)]
pub struct SmsService<'a> {
    client: &'a Kavenegar,
}

impl<'a> SmsService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Sends one message to one or more recipients.
    ///
    /// Await the returned builder to run the request; set optional parameters
    /// on it first.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let sent = client
    ///     .sms()
    ///     .send(["09177788155", "09131234567"], "کد شما: ۱۲۳۴")
    ///     .sender("10004346")
    ///     .tag("signup")
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send(
        self,
        receptors: impl IntoIterator<Item = impl Into<String>>,
        message: impl Into<String>,
    ) -> SendBuilder<'a> {
        SendBuilder {
            client: self.client,
            receptors: receptors.into_iter().map(Into::into).collect(),
            message: message.into(),
            sender: None,
            date: None,
            message_type: None,
            local_ids: Vec::new(),
            hide: false,
            tag: None,
            policy: None,
            media_id: None,
        }
    }

    /// Sends a distinct message to each recipient in a single call.
    ///
    /// The recipient and message lists must be the same length, and a sender
    /// is required — set one line for all with [`SendArrayBuilder::sender`]
    /// or one per message with [`SendArrayBuilder::senders`]. Mistakes are
    /// rejected locally before any request is sent.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let sent = client
    ///     .sms()
    ///     .send_array(
    ///         ["09177788155", "09131234567"],
    ///         ["پیام اول", "پیام دوم"],
    ///     )
    ///     .sender("10004346")
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_array(
        self,
        receptors: impl IntoIterator<Item = impl Into<String>>,
        messages: impl IntoIterator<Item = impl Into<String>>,
    ) -> SendArrayBuilder<'a> {
        SendArrayBuilder {
            client: self.client,
            receptors: receptors.into_iter().map(Into::into).collect(),
            messages: messages.into_iter().map(Into::into).collect(),
            senders: Vec::new(),
            date: None,
            message_types: Vec::new(),
            local_ids: Vec::new(),
            hide: false,
            tag: None,
            policy: None,
            media_id: None,
        }
    }

    /// Looks up the delivery status of previously sent messages.
    ///
    /// Up to 500 identifiers per call. Only messages from the last 48 hours
    /// are reported; older ones come back as [`MessageStatus::Invalid`].
    ///
    /// [`MessageStatus::Invalid`]: crate::MessageStatus::Invalid
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`], before
    /// anything is sent, if `message_ids` is empty or holds more than 500 ids. Otherwise returns
    /// [`Error::Api`] when Kavenegar refuses the request, or
    /// another [`Error`] if it cannot be sent or decoded.
    pub async fn status(self, message_ids: impl IntoIterator<Item = i64>) -> Result<Vec<Message>> {
        let ids = message_id_list(message_ids)?;
        self.client
            .call_list(Verb::Post, MODULE, "status", vec![("messageid", ids)])
            .await
    }

    /// Looks up delivery status by the local identifiers you supplied at send
    /// time, for messages sent within the last 12 hours.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`], before
    /// anything is sent, if `local_ids` is empty. Otherwise returns
    /// [`Error::Api`] when Kavenegar refuses the request, or
    /// another [`Error`] if it cannot be sent or decoded.
    pub async fn status_by_local_id(
        self,
        local_ids: impl IntoIterator<Item = i64>,
    ) -> Result<Vec<Message>> {
        let ids = join_ids(local_ids);
        if ids.is_empty() {
            return Err(Error::invalid("local_ids", "must not be empty"));
        }
        self.client
            .call_list(
                Verb::Post,
                MODULE,
                "statuslocalmessageid",
                vec![("localid", ids)],
            )
            .await
    }

    /// Looks up delivery status for everything sent to one recipient in a
    /// time window, starting at a Unix timestamp in seconds.
    ///
    /// The window may not exceed one day, and defaults to one day when no end
    /// is given.
    pub fn status_by_receptor(
        self,
        receptor: impl Into<String>,
        start_date: i64,
    ) -> StatusByReceptorBuilder<'a> {
        StatusByReceptorBuilder {
            client: self.client,
            receptor: receptor.into(),
            start_date,
            end_date: None,
        }
    }

    /// Fetches full records, including body and cost, for the given messages.
    ///
    /// Up to 500 identifiers per call. Needs an IP allowlist entry in the
    /// panel, or fails with [`ApiStatus::AccessDenied`].
    ///
    /// [`ApiStatus::AccessDenied`]: crate::ApiStatus::AccessDenied
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`], before
    /// anything is sent, if `message_ids` is empty or holds more than 500 ids. Otherwise returns
    /// [`Error::Api`] when Kavenegar refuses the request, or
    /// another [`Error`] if it cannot be sent or decoded.
    pub async fn select(self, message_ids: impl IntoIterator<Item = i64>) -> Result<Vec<Message>> {
        let ids = message_id_list(message_ids)?;
        self.client
            .call_list(Verb::Post, MODULE, "select", vec![("messageid", ids)])
            .await
    }

    /// Lists sent messages in a time window, as Unix timestamps in seconds.
    ///
    /// The window may not exceed one day, may start at most three days ago,
    /// and returns at most 500 records. Needs an IP allowlist entry in the
    /// panel, or fails with [`ApiStatus::AccessDenied`].
    ///
    /// [`ApiStatus::AccessDenied`]: crate::ApiStatus::AccessDenied
    pub fn select_outbox(self, start_date: i64) -> SelectOutboxBuilder<'a> {
        SelectOutboxBuilder {
            client: self.client,
            start_date,
            end_date: None,
            sender: None,
        }
    }

    /// Lists the most recently sent messages, newest first.
    ///
    /// Needs an IP allowlist entry in the panel, or fails with
    /// [`ApiStatus::AccessDenied`].
    ///
    /// [`ApiStatus::AccessDenied`]: crate::ApiStatus::AccessDenied
    pub fn latest_outbox(self) -> LatestOutboxBuilder<'a> {
        LatestOutboxBuilder {
            client: self.client,
            page_size: None,
            sender: None,
        }
    }

    /// Counts sent messages, and their cost, in a time window.
    pub fn count_outbox(self, start_date: i64) -> CountOutboxBuilder<'a> {
        CountOutboxBuilder {
            client: self.client,
            start_date,
            end_date: None,
            status: None,
        }
    }

    /// Cancels scheduled messages, refunding their cost.
    ///
    /// Only a message sent with a future date can be cancelled. Up to 500
    /// identifiers per call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`], before
    /// anything is sent, if `message_ids` is empty or holds more than 500 ids. Otherwise returns
    /// [`Error::Api`] when Kavenegar refuses the request, or
    /// another [`Error`] if it cannot be sent or decoded.
    pub async fn cancel(self, message_ids: impl IntoIterator<Item = i64>) -> Result<Vec<Message>> {
        let ids = message_id_list(message_ids)?;
        self.client
            .call_list(Verb::Post, MODULE, "cancel", vec![("messageid", ids)])
            .await
    }

    /// Fetches messages received on one of your lines, 100 at a time.
    ///
    /// By default this fetches unread messages, and Kavenegar marks every
    /// message it returns as read — so a second call will not repeat them.
    /// Keep calling while a full batch of 100 comes back.
    pub fn receive(self, line_number: impl Into<String>) -> ReceiveBuilder<'a> {
        ReceiveBuilder {
            client: self.client,
            line_number: line_number.into(),
            unread: true,
        }
    }

    /// Fetches received messages a page at a time, 200 per page.
    ///
    /// Like [`SmsService::receive`], Kavenegar marks every message it returns
    /// as read, so asking for [`InboxPagedBuilder::unread`] twice does not
    /// return the same messages twice. What this adds is a date window and
    /// page counters. The window defaults to the last two days, which is also
    /// the widest allowed.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let page = client.sms().inbox_paged("3000202030").unread().await?;
    /// println!("{} of {}", page.len(), page.total_count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn inbox_paged(self, line_number: impl Into<String>) -> InboxPagedBuilder<'a> {
        InboxPagedBuilder {
            client: self.client,
            line_number: line_number.into(),
            is_read: None,
            start_date: None,
            end_date: None,
            page_number: None,
        }
    }

    /// Counts messages received on a line in a time window.
    pub fn count_inbox(self, start_date: i64) -> CountInboxBuilder<'a> {
        CountInboxBuilder {
            client: self.client,
            start_date,
            end_date: None,
            line_number: None,
            read_unread: None,
        }
    }
}

/// Builds a call to `sms/send`. Await it to send the message.
///
/// `Debug` shows counts and lengths, not the recipients or the text, which may
/// be a one-time password.
#[must_use = "the message is only sent when the builder is awaited"]
pub struct SendBuilder<'a> {
    client: &'a Kavenegar,
    receptors: Vec<String>,
    message: String,
    sender: Option<String>,
    date: Option<i64>,
    message_type: Option<MessageType>,
    local_ids: Vec<i64>,
    hide: bool,
    tag: Option<String>,
    policy: Option<String>,
    media_id: Option<String>,
}

impl std::fmt::Debug for SendBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendBuilder")
            .field("recipients", &self.receptors.len())
            .field("message_chars", &self.message.chars().count())
            .field("sender", &self.sender)
            .field("date", &self.date)
            .field("message_type", &self.message_type)
            .field("local_ids", &self.local_ids.len())
            .field("hide", &self.hide)
            .field("tag", &self.tag)
            .field("policy", &self.policy)
            .field("media_id", &self.media_id)
            .finish_non_exhaustive()
    }
}

impl<'a> SendBuilder<'a> {
    /// Sends from a specific line, instead of the account default.
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }

    /// Schedules the message for a Unix timestamp in seconds.
    pub fn schedule_at(mut self, date: i64) -> Self {
        self.date = Some(date);
        self
    }

    /// Chooses how the handset presents the message. Only `3000` lines
    /// honour this.
    pub fn message_type(mut self, message_type: MessageType) -> Self {
        self.message_type = Some(message_type);
        self
    }

    /// Attaches your own numeric identifiers, so status can later be looked
    /// up with [`SmsService::status_by_local_id`].
    ///
    /// Supply exactly one per recipient. Kavenegar treats a repeated local id
    /// as a duplicate: it does not send again, and returns the earlier record.
    pub fn local_ids(mut self, local_ids: impl IntoIterator<Item = i64>) -> Self {
        self.local_ids = local_ids.into_iter().collect();
        self
    }

    /// Hides the recipient number in the Kavenegar panel and reports.
    pub fn hide_recipient(mut self, hide: bool) -> Self {
        self.hide = hide;
        self
    }

    /// Tags the message, for grouping in reports.
    ///
    /// The tag must already exist in your panel, or Kavenegar rejects the
    /// send with [`ApiStatus::InvalidTag`].
    ///
    /// [`ApiStatus::InvalidTag`]: crate::ApiStatus::InvalidTag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Sends through a named sending policy defined on your account, rather
    /// than a single line.
    pub fn policy(mut self, policy: impl Into<String>) -> Self {
        self.policy = Some(policy.into());
        self
    }

    /// Attaches an uploaded image or video, by the id `media().upload()`
    /// returned.
    ///
    /// Only domestic messenger lines can carry media; with a policy, every
    /// line in it must be one.
    pub fn media_id(mut self, media_id: impl Into<String>) -> Self {
        self.media_id = Some(media_id.into());
        self
    }

    fn into_params(self) -> Result<(&'a Kavenegar, Vec<(&'static str, String)>)> {
        check_recipient_count(self.receptors.len())?;
        check::numbers("receptors", &self.receptors)?;
        if self.message.trim().is_empty() {
            return Err(Error::invalid("message", "must not be empty"));
        }
        if let Some(sender) = &self.sender {
            check::number("sender", sender)?;
        }
        if let Some(tag) = &self.tag {
            check::tag(tag)?;
        }
        if !self.local_ids.is_empty() && self.local_ids.len() != self.receptors.len() {
            return Err(Error::invalid(
                "local_ids",
                format!(
                    "expected one per recipient: {} recipients but {} local ids",
                    self.receptors.len(),
                    self.local_ids.len()
                ),
            ));
        }

        let mut params = vec![
            ("receptor", self.receptors.join(",")),
            ("message", self.message),
        ];
        if let Some(sender) = self.sender {
            params.push(("sender", sender));
        }
        if let Some(date) = self.date {
            params.push(("date", date.to_string()));
        }
        if let Some(message_type) = self.message_type {
            params.push(("type", message_type.code().to_owned()));
        }
        if !self.local_ids.is_empty() {
            params.push(("localid", join_ids(self.local_ids)));
        }
        if self.hide {
            params.push(("hide", "1".to_owned()));
        }
        if let Some(tag) = self.tag {
            params.push(("tag", tag));
        }
        if let Some(policy) = self.policy {
            params.push(("policy", policy));
        }
        if let Some(media_id) = self.media_id {
            params.push(("mediaid", media_id));
        }
        Ok((self.client, params))
    }
}

impl<'a> IntoFuture for SendBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let (client, params) = self.into_params()?;
            client.call_list(Verb::Post, MODULE, "send", params).await
        })
    }
}

/// Builds a call to `sms/sendarray`. Await it to send the messages.
///
/// `Debug` shows counts, not the recipients or the texts.
#[must_use = "the messages are only sent when the builder is awaited"]
pub struct SendArrayBuilder<'a> {
    client: &'a Kavenegar,
    receptors: Vec<String>,
    messages: Vec<String>,
    senders: Vec<String>,
    date: Option<i64>,
    message_types: Vec<MessageType>,
    local_ids: Vec<i64>,
    hide: bool,
    tag: Option<String>,
    policy: Option<String>,
    media_id: Option<String>,
}

impl std::fmt::Debug for SendArrayBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendArrayBuilder")
            .field("recipients", &self.receptors.len())
            .field("messages", &self.messages.len())
            .field("senders", &self.senders)
            .field("date", &self.date)
            .field("message_types", &self.message_types)
            .field("local_ids", &self.local_ids.len())
            .field("hide", &self.hide)
            .field("tag", &self.tag)
            .field("policy", &self.policy)
            .field("media_id", &self.media_id)
            .finish_non_exhaustive()
    }
}

impl<'a> SendArrayBuilder<'a> {
    /// Sends every message from one line.
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        let sender = sender.into();
        self.senders = vec![sender; self.receptors.len().max(1)];
        self
    }

    /// Sends each message from its own line, one per recipient.
    pub fn senders(mut self, senders: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.senders = senders.into_iter().map(Into::into).collect();
        self
    }

    /// Schedules the batch for a Unix timestamp in seconds.
    pub fn schedule_at(mut self, date: i64) -> Self {
        self.date = Some(date);
        self
    }

    /// Applies one presentation type to every message.
    pub fn message_type(mut self, message_type: MessageType) -> Self {
        self.message_types = vec![message_type; self.receptors.len().max(1)];
        self
    }

    /// Sets a presentation type per message.
    pub fn message_types(mut self, types: impl IntoIterator<Item = MessageType>) -> Self {
        self.message_types = types.into_iter().collect();
        self
    }

    /// Attaches your own numeric identifiers, one per recipient.
    ///
    /// As with [`SendBuilder::local_ids`], a repeated id is not sent again.
    pub fn local_ids(mut self, local_ids: impl IntoIterator<Item = i64>) -> Self {
        self.local_ids = local_ids.into_iter().collect();
        self
    }

    /// Hides the recipient numbers in the Kavenegar panel and reports.
    pub fn hide_recipient(mut self, hide: bool) -> Self {
        self.hide = hide;
        self
    }

    /// Tags the batch, for grouping in reports. The tag must already exist
    /// in your panel.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Sends through a named sending policy defined on your account.
    pub fn policy(mut self, policy: impl Into<String>) -> Self {
        self.policy = Some(policy.into());
        self
    }

    /// Attaches an uploaded image or video to every message in the batch.
    /// Only domestic messenger lines can carry media.
    pub fn media_id(mut self, media_id: impl Into<String>) -> Self {
        self.media_id = Some(media_id.into());
        self
    }

    fn into_params(self) -> Result<(&'a Kavenegar, Vec<(&'static str, String)>)> {
        check_recipient_count(self.receptors.len())?;
        check::numbers("receptors", &self.receptors)?;
        if self.receptors.len() != self.messages.len() {
            return Err(Error::invalid(
                "messages",
                format!(
                    "must match the recipient count: {} recipients but {} messages",
                    self.receptors.len(),
                    self.messages.len()
                ),
            ));
        }
        if self.messages.iter().any(|m| m.trim().is_empty()) {
            return Err(Error::invalid("messages", "no message may be empty"));
        }
        if !self.message_types.is_empty() && self.message_types.len() != self.receptors.len() {
            return Err(Error::invalid(
                "message_types",
                format!(
                    "must match the recipient count: {} recipients but {} types",
                    self.receptors.len(),
                    self.message_types.len()
                ),
            ));
        }
        // Unlike `send`, `sendarray` documents the sender as mandatory.
        if self.senders.is_empty() {
            return Err(Error::invalid(
                "senders",
                "send_array requires a sender; set one with `sender` or `senders`",
            ));
        }
        if self.senders.len() != self.receptors.len() {
            return Err(Error::invalid(
                "senders",
                format!(
                    "must match the recipient count: {} recipients but {} senders",
                    self.receptors.len(),
                    self.senders.len()
                ),
            ));
        }
        check::numbers("senders", &self.senders)?;
        if let Some(tag) = &self.tag {
            check::tag(tag)?;
        }
        if !self.local_ids.is_empty() && self.local_ids.len() != self.receptors.len() {
            return Err(Error::invalid(
                "local_ids",
                format!(
                    "must match the recipient count: {} recipients but {} local ids",
                    self.receptors.len(),
                    self.local_ids.len()
                ),
            ));
        }

        // sendarray takes JSON arrays rather than comma-separated lists.
        let mut params = vec![
            ("receptor", json_array(&self.receptors)),
            ("sender", json_array(&self.senders)),
            ("message", json_array(&self.messages)),
        ];
        if let Some(date) = self.date {
            params.push(("date", date.to_string()));
        }
        if !self.message_types.is_empty() {
            let codes: Vec<String> = self
                .message_types
                .iter()
                .map(|t| t.code().to_owned())
                .collect();
            params.push(("type", format!("[{}]", codes.join(","))));
        }
        if !self.local_ids.is_empty() {
            // Documented as an array of longs, under a different name than
            // `send` uses.
            params.push(("localmessageids", format!("[{}]", join_ids(self.local_ids))));
        }
        if self.hide {
            params.push(("hide", "1".to_owned()));
        }
        if let Some(tag) = self.tag {
            params.push(("tag", tag));
        }
        if let Some(policy) = self.policy {
            params.push(("policy", policy));
        }
        if let Some(media_id) = self.media_id {
            params.push(("mediaid", media_id));
        }
        Ok((self.client, params))
    }
}

impl<'a> IntoFuture for SendArrayBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let (client, params) = self.into_params()?;
            client
                .call_list(Verb::Post, MODULE, "sendarray", params)
                .await
        })
    }
}

/// Builds a call to `sms/statusbyreceptor`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct StatusByReceptorBuilder<'a> {
    client: &'a Kavenegar,
    receptor: String,
    start_date: i64,
    end_date: Option<i64>,
}

impl<'a> StatusByReceptorBuilder<'a> {
    /// Ends the window at this Unix timestamp, at most one day after the start.
    pub fn end_date(mut self, end_date: i64) -> Self {
        self.end_date = Some(end_date);
        self
    }
}

impl<'a> IntoFuture for StatusByReceptorBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            check::number("receptor", &self.receptor)?;
            check_window(self.start_date, self.end_date)?;

            let mut params = vec![
                ("receptor", self.receptor),
                ("startdate", self.start_date.to_string()),
            ];
            if let Some(end_date) = self.end_date {
                params.push(("enddate", end_date.to_string()));
            }
            self.client
                .call_list(Verb::Post, MODULE, "statusbyreceptor", params)
                .await
        })
    }
}

/// Builds a call to `sms/selectoutbox`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct SelectOutboxBuilder<'a> {
    client: &'a Kavenegar,
    start_date: i64,
    end_date: Option<i64>,
    sender: Option<String>,
}

impl<'a> SelectOutboxBuilder<'a> {
    /// Ends the window at this Unix timestamp. Defaults to the current time.
    pub fn end_date(mut self, end_date: i64) -> Self {
        self.end_date = Some(end_date);
        self
    }

    /// Restricts the results to one line.
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }
}

impl<'a> IntoFuture for SelectOutboxBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            check_window(self.start_date, self.end_date)?;
            if let Some(sender) = &self.sender {
                check::number("sender", sender)?;
            }
            let mut params = vec![("startdate", self.start_date.to_string())];
            if let Some(end_date) = self.end_date {
                params.push(("enddate", end_date.to_string()));
            }
            if let Some(sender) = self.sender {
                params.push(("sender", sender));
            }
            self.client
                .call_list(Verb::Post, MODULE, "selectoutbox", params)
                .await
        })
    }
}

/// Builds a call to `sms/latestoutbox`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct LatestOutboxBuilder<'a> {
    client: &'a Kavenegar,
    page_size: Option<u32>,
    sender: Option<String>,
}

impl<'a> LatestOutboxBuilder<'a> {
    /// Limits how many records are returned. Defaults to, and is capped at,
    /// 500.
    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = Some(page_size);
        self
    }

    /// Restricts the results to one line.
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }
}

impl<'a> IntoFuture for LatestOutboxBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            if let Some(page_size) = self.page_size
                && !(1..=MAX_OUTBOX_PAGE).contains(&page_size)
            {
                return Err(Error::invalid(
                    "page_size",
                    format!("must be between 1 and {MAX_OUTBOX_PAGE}"),
                ));
            }
            if let Some(sender) = &self.sender {
                check::number("sender", sender)?;
            }
            let mut params = Vec::new();
            if let Some(page_size) = self.page_size {
                params.push(("pagesize", page_size.to_string()));
            }
            if let Some(sender) = self.sender {
                params.push(("sender", sender));
            }
            self.client
                .call_list(Verb::Post, MODULE, "latestoutbox", params)
                .await
        })
    }
}

/// Builds a call to `sms/countoutbox`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct CountOutboxBuilder<'a> {
    client: &'a Kavenegar,
    start_date: i64,
    end_date: Option<i64>,
    status: Option<i64>,
}

impl<'a> CountOutboxBuilder<'a> {
    /// Ends the window at this Unix timestamp. Defaults to the current time.
    pub fn end_date(mut self, end_date: i64) -> Self {
        self.end_date = Some(end_date);
        self
    }

    /// Counts only messages in one delivery state.
    pub fn status(mut self, status: crate::models::MessageStatus) -> Self {
        self.status = Some(status.code());
        self
    }
}

impl<'a> IntoFuture for CountOutboxBuilder<'a> {
    type Output = Result<OutboxCount>;
    type IntoFuture = BoxFuture<'a, Result<OutboxCount>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            check_window(self.start_date, self.end_date)?;
            let mut params = vec![("startdate", self.start_date.to_string())];
            if let Some(end_date) = self.end_date {
                params.push(("enddate", end_date.to_string()));
            }
            if let Some(status) = self.status {
                params.push(("status", status.to_string()));
            }
            self.client
                .call_one(Verb::Post, MODULE, "countoutbox", params)
                .await
        })
    }
}

/// Builds a call to `sms/receive`. Await it to fetch the messages.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct ReceiveBuilder<'a> {
    client: &'a Kavenegar,
    line_number: String,
    unread: bool,
}

impl<'a> ReceiveBuilder<'a> {
    /// Fetches unread messages and marks them read. This is the default.
    pub fn unread(mut self) -> Self {
        self.unread = true;
        self
    }

    /// Fetches messages already marked read.
    pub fn already_read(mut self) -> Self {
        self.unread = false;
        self
    }
}

impl<'a> IntoFuture for ReceiveBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            check::number("line_number", &self.line_number)?;
            let params = vec![
                ("linenumber", self.line_number),
                ("isread", if self.unread { "0" } else { "1" }.to_owned()),
            ];
            self.client
                .call_list(Verb::Post, MODULE, "receive", params)
                .await
        })
    }
}

/// Builds a call to `sms/inboxpaged`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct InboxPagedBuilder<'a> {
    client: &'a Kavenegar,
    line_number: String,
    is_read: Option<bool>,
    start_date: Option<i64>,
    end_date: Option<i64>,
    page_number: Option<u32>,
}

impl<'a> InboxPagedBuilder<'a> {
    /// Fetches only messages not yet marked read.
    pub fn unread(mut self) -> Self {
        self.is_read = Some(false);
        self
    }

    /// Fetches only messages already marked read.
    pub fn already_read(mut self) -> Self {
        self.is_read = Some(true);
        self
    }

    /// Starts the window at this Unix timestamp.
    ///
    /// Without a start, Kavenegar reads the last two days, which is also the
    /// widest window allowed.
    pub fn start_date(mut self, start_date: i64) -> Self {
        self.start_date = Some(start_date);
        self
    }

    /// Ends the window at this Unix timestamp, at most two days after the start.
    pub fn end_date(mut self, end_date: i64) -> Self {
        self.end_date = Some(end_date);
        self
    }

    /// Selects a page, counting from 1.
    pub fn page(mut self, page_number: u32) -> Self {
        self.page_number = Some(page_number);
        self
    }
}

impl<'a> IntoFuture for InboxPagedBuilder<'a> {
    type Output = Result<Page<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Page<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            check::number("line_number", &self.line_number)?;
            if let Some(start) = self.start_date {
                check_window(start, self.end_date)?;
            }

            let mut params = vec![("linenumber", self.line_number)];
            if let Some(is_read) = self.is_read {
                params.push(("isread", if is_read { "1" } else { "0" }.to_owned()));
            }
            if let Some(start_date) = self.start_date {
                params.push(("startdate", start_date.to_string()));
            }
            if let Some(end_date) = self.end_date {
                params.push(("enddate", end_date.to_string()));
            }
            if let Some(page_number) = self.page_number {
                params.push(("pagenumber", page_number.to_string()));
            }
            self.client
                .call_page(Verb::Post, MODULE, "inboxpaged", params)
                .await
        })
    }
}

/// Builds a call to `sms/countinbox`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct CountInboxBuilder<'a> {
    client: &'a Kavenegar,
    start_date: i64,
    end_date: Option<i64>,
    line_number: Option<String>,
    read_unread: Option<bool>,
}

impl<'a> CountInboxBuilder<'a> {
    /// Ends the window at this Unix timestamp. Defaults to the current time.
    pub fn end_date(mut self, end_date: i64) -> Self {
        self.end_date = Some(end_date);
        self
    }

    /// Counts only messages received on one line.
    pub fn line_number(mut self, line_number: impl Into<String>) -> Self {
        self.line_number = Some(line_number.into());
        self
    }

    /// Counts only read, or only unread, messages.
    pub fn is_read(mut self, is_read: bool) -> Self {
        self.read_unread = Some(is_read);
        self
    }
}

impl<'a> IntoFuture for CountInboxBuilder<'a> {
    type Output = Result<InboxCount>;
    type IntoFuture = BoxFuture<'a, Result<InboxCount>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            check_window(self.start_date, self.end_date)?;
            if let Some(line_number) = &self.line_number {
                check::number("line_number", line_number)?;
            }
            let mut params = vec![("startdate", self.start_date.to_string())];
            if let Some(end_date) = self.end_date {
                params.push(("enddate", end_date.to_string()));
            }
            if let Some(line_number) = self.line_number {
                params.push(("linenumber", line_number));
            }
            if let Some(is_read) = self.read_unread {
                params.push(("isread", if is_read { "1" } else { "0" }.to_owned()));
            }
            self.client
                .call_one(Verb::Post, MODULE, "countinbox", params)
                .await
        })
    }
}

/// How the handset should present a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum MessageType {
    /// Shown immediately on screen, not stored.
    Flash,
    /// Stored in phone memory. The API default.
    #[default]
    PhoneMemory,
    /// Stored in SIM memory.
    SimMemory,
    /// Delivered to the app inbox.
    AppMemory,
}

impl MessageType {
    pub(crate) fn code(self) -> &'static str {
        match self {
            MessageType::Flash => "0",
            MessageType::PhoneMemory => "1",
            MessageType::SimMemory => "2",
            MessageType::AppMemory => "3",
        }
    }
}

/// Aggregate counters returned by `sms/countoutbox`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct OutboxCount {
    /// Start of the counted window, as a Unix timestamp.
    #[serde(rename = "startdate", default, deserialize_with = "lenient_i64")]
    pub start_date: i64,
    /// End of the counted window, as a Unix timestamp.
    #[serde(rename = "enddate", default, deserialize_with = "lenient_i64")]
    pub end_date: i64,
    /// Total number of message parts.
    #[serde(rename = "sumpart", default, deserialize_with = "lenient_i64")]
    pub sum_part: i64,
    /// Total number of messages.
    #[serde(rename = "sumcount", default, deserialize_with = "lenient_i64")]
    pub sum_count: i64,
    /// Total cost in Rials.
    #[serde(default, deserialize_with = "lenient_i64")]
    pub cost: i64,
}

/// Aggregate counters returned by `sms/countinbox`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct InboxCount {
    /// Start of the counted window, as a Unix timestamp.
    #[serde(rename = "startdate", default, deserialize_with = "lenient_i64")]
    pub start_date: i64,
    /// End of the counted window, as a Unix timestamp.
    #[serde(rename = "enddate", default, deserialize_with = "lenient_i64")]
    pub end_date: i64,
    /// Total number of messages received.
    #[serde(rename = "sumcount", default, deserialize_with = "lenient_i64")]
    pub sum_count: i64,
}

fn join_ids(ids: impl IntoIterator<Item = i64>) -> String {
    ids.into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Validates and joins the ids for status, select and cancel.
fn message_id_list(ids: impl IntoIterator<Item = i64>) -> Result<String> {
    let ids: Vec<i64> = ids.into_iter().collect();
    if ids.is_empty() {
        return Err(Error::invalid("message_ids", "must not be empty"));
    }
    if ids.len() > MAX_MESSAGE_IDS {
        return Err(Error::invalid(
            "message_ids",
            format!("at most {MAX_MESSAGE_IDS} per call, got {}", ids.len()),
        ));
    }
    Ok(join_ids(ids))
}

fn check_recipient_count(count: usize) -> Result<()> {
    if count == 0 {
        return Err(Error::invalid(
            "receptors",
            "at least one recipient is required",
        ));
    }
    if count > MAX_RECIPIENTS {
        return Err(Error::invalid(
            "receptors",
            format!("at most {MAX_RECIPIENTS} recipients per call, got {count}"),
        ));
    }
    Ok(())
}

/// Rejects a window that ends before it starts, which Kavenegar answers with
/// a 417. The longest allowed window differs per endpoint, so that check is
/// left to the server.
fn check_window(start: i64, end: Option<i64>) -> Result<()> {
    match end {
        Some(end) if end < start => Err(Error::invalid(
            "end_date",
            "must not be earlier than start_date",
        )),
        _ => Ok(()),
    }
}

/// Renders a JSON string array, which `sendarray` expects for its list fields.
fn json_array(values: &[String]) -> String {
    serde_json::Value::Array(
        values
            .iter()
            .map(|v| serde_json::Value::String(v.clone()))
            .collect(),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Kavenegar {
        Kavenegar::new("test-key")
    }

    #[test]
    fn send_rejects_an_empty_recipient_list() {
        let c = client();
        let err = c
            .sms()
            .send(Vec::<String>::new(), "hi")
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "receptors",
                ..
            }
        ));
    }

    #[test]
    fn send_rejects_a_blank_message() {
        let c = client();
        let err = c.sms().send(["0912"], "   ").into_params().unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "message",
                ..
            }
        ));
    }

    #[test]
    fn send_rejects_mismatched_local_ids() {
        let c = client();
        let err = c
            .sms()
            .send(["0912", "0913"], "hi")
            .local_ids([1])
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "local_ids",
                ..
            }
        ));
    }

    #[test]
    fn send_builds_the_expected_form() {
        let c = client();
        let (_, params) = c
            .sms()
            .send(["0912", "0913"], "سلام")
            .sender("10004346")
            .message_type(MessageType::Flash)
            .tag("signup")
            .hide_recipient(true)
            .into_params()
            .unwrap();

        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("receptor"), Some("0912,0913"));
        assert_eq!(get("message"), Some("سلام"));
        assert_eq!(get("sender"), Some("10004346"));
        assert_eq!(get("type"), Some("0"));
        assert_eq!(get("tag"), Some("signup"));
        assert_eq!(get("hide"), Some("1"));
        assert_eq!(get("date"), None);
    }

    #[test]
    fn a_recipient_cannot_smuggle_in_more_recipients() {
        let c = client();
        let err = c
            .sms()
            .send(["09121234567,09131234567"], "hi")
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "receptors",
                ..
            }
        ));
    }

    #[test]
    fn send_array_requires_a_sender() {
        let c = client();
        let err = c
            .sms()
            .send_array(["0912"], ["hi"])
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "senders",
                ..
            }
        ));
    }

    #[test]
    fn send_rejects_a_malformed_tag() {
        let c = client();
        assert!(
            c.sms()
                .send(["0912"], "hi")
                .tag("has space")
                .into_params()
                .is_err()
        );
    }

    #[test]
    fn send_array_requires_matching_lengths() {
        let c = client();
        let err = c
            .sms()
            .send_array(["0912", "0913"], ["only one message"])
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "messages",
                ..
            }
        ));
    }

    #[test]
    fn send_array_encodes_json_arrays() {
        let c = client();
        let (_, params) = c
            .sms()
            .send_array(["0912", "0913"], ["یک", "دو"])
            .sender("10004346")
            .into_params()
            .unwrap();

        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("receptor"), Some(r#"["0912","0913"]"#));
        assert_eq!(get("message"), Some(r#"["یک","دو"]"#));
        assert_eq!(get("sender"), Some(r#"["10004346","10004346"]"#));
    }

    #[test]
    fn send_passes_policy_media_and_numeric_local_ids() {
        let c = client();
        let (_, params) = c
            .sms()
            .send(["0912", "0913"], "hi")
            .local_ids([7, 8])
            .policy("night-flow")
            .media_id("3fa85f64-5717-4562-b3fc-2c963f66afa6")
            .into_params()
            .unwrap();

        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("localid"), Some("7,8"));
        assert_eq!(get("policy"), Some("night-flow"));
        assert_eq!(get("mediaid"), Some("3fa85f64-5717-4562-b3fc-2c963f66afa6"));
    }

    #[test]
    fn send_array_names_its_local_ids_as_documented() {
        let c = client();
        let (_, params) = c
            .sms()
            .send_array(["0912", "0913"], ["a", "b"])
            .sender("10004346")
            .local_ids([1, 2])
            .message_type(MessageType::Flash)
            .into_params()
            .unwrap();

        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("localmessageids"), Some("[1,2]"));
        assert_eq!(get("localid"), None);
        assert_eq!(get("type"), Some("[0,0]"));
    }

    #[test]
    fn send_enforces_the_two_hundred_recipient_cap() {
        let c = client();
        let many: Vec<String> = (0..201).map(|i| format!("0912{i:07}")).collect();
        let err = c.sms().send(many, "hi").into_params().unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "receptors",
                ..
            }
        ));
    }

    #[test]
    fn id_lists_are_capped_at_five_hundred() {
        assert!(message_id_list(0..500).is_ok());
        assert!(message_id_list(0..501).is_err());
        assert!(message_id_list(std::iter::empty()).is_err());
    }

    #[test]
    fn a_backwards_window_is_rejected() {
        assert!(check_window(100, Some(99)).is_err());
        assert!(check_window(100, Some(100)).is_ok());
        assert!(check_window(100, None).is_ok());
    }

    #[test]
    fn debug_shows_no_recipient_or_message_text() {
        let c = client();
        let send = c.sms().send(["09121234567"], "your code is 48391");
        let shown = format!("{send:?}");
        assert!(
            !shown.contains("48391") && !shown.contains("09121234567"),
            "{shown}"
        );

        let batch = c.sms().send_array(["09121234567"], ["code 48391"]);
        let shown = format!("{batch:?}");
        assert!(
            !shown.contains("48391") && !shown.contains("09121234567"),
            "{shown}"
        );
    }

    #[test]
    fn json_array_escapes_quotes() {
        let values = vec![r#"say "hi""#.to_owned()];
        assert_eq!(json_array(&values), r#"["say \"hi\""]"#);
    }
}
