//! Text-to-speech voice calls.

use std::future::IntoFuture;

use crate::check;
use crate::client::{BoxFuture, Kavenegar, Verb};
use crate::error::{Error, Result};
use crate::models::Message;

const MODULE: &str = "call";

/// The most recipients Kavenegar accepts in one call request.
const MAX_RECIPIENTS: usize = 200;

/// The most redial attempts Kavenegar documents for an unanswered call.
const MAX_REPEAT: u8 = 5;

/// Entry point for the `call` endpoints, from [`Kavenegar::call`].
#[derive(Debug, Clone, Copy)]
pub struct CallService<'a> {
    client: &'a Kavenegar,
}

impl<'a> CallService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Places a call that reads the given text aloud, to up to 200
    /// recipients.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let calls = client
    ///     .call()
    ///     .text_to_speech(["09177788155"], "سفارش شما ارسال شد")
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn text_to_speech(
        self,
        receptors: impl IntoIterator<Item = impl Into<String>>,
        message: impl Into<String>,
    ) -> TextToSpeechBuilder<'a> {
        TextToSpeechBuilder {
            client: self.client,
            receptors: receptors.into_iter().map(Into::into).collect(),
            message: message.into(),
            date: None,
            local_ids: Vec::new(),
            repeat: None,
            tag: None,
        }
    }
}

/// Builds a call to `call/maketts`. Await it to place the call.
///
/// `Debug` shows counts and lengths, not the recipients or the text.
#[must_use = "the call is only placed when the builder is awaited"]
pub struct TextToSpeechBuilder<'a> {
    client: &'a Kavenegar,
    receptors: Vec<String>,
    message: String,
    date: Option<i64>,
    local_ids: Vec<String>,
    repeat: Option<u8>,
    tag: Option<String>,
}

impl std::fmt::Debug for TextToSpeechBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextToSpeechBuilder")
            .field("recipients", &self.receptors.len())
            .field("message_chars", &self.message.chars().count())
            .field("date", &self.date)
            .field("local_ids", &self.local_ids.len())
            .field("repeat", &self.repeat)
            .field("tag", &self.tag)
            .finish_non_exhaustive()
    }
}

impl<'a> TextToSpeechBuilder<'a> {
    /// Schedules the call for a Unix timestamp in seconds.
    pub fn schedule_at(mut self, date: i64) -> Self {
        self.date = Some(date);
        self
    }

    /// Attaches your own identifiers, one per recipient. A repeated id is
    /// treated as a duplicate and not called again.
    pub fn local_ids(mut self, local_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.local_ids = local_ids.into_iter().map(Into::into).collect();
        self
    }

    /// Redials an unanswered call up to this many times, 0 to 5, three
    /// minutes apart.
    ///
    /// Kavenegar documents this parameter as currently disabled, so it is
    /// sent but may have no effect.
    pub fn repeat(mut self, times: u8) -> Self {
        self.repeat = Some(times);
        self
    }

    /// Tags the call, for grouping in reports. The tag must already exist in
    /// your panel.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    fn into_params(self) -> Result<(&'a Kavenegar, Vec<(&'static str, String)>)> {
        if self.receptors.is_empty() {
            return Err(Error::invalid(
                "receptors",
                "at least one recipient is required",
            ));
        }
        if self.receptors.len() > MAX_RECIPIENTS {
            return Err(Error::invalid(
                "receptors",
                format!(
                    "at most {MAX_RECIPIENTS} recipients per call, got {}",
                    self.receptors.len()
                ),
            ));
        }
        check::numbers("receptors", &self.receptors)?;
        if self.message.trim().is_empty() {
            return Err(Error::invalid("message", "must not be empty"));
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
        if let Some(repeat) = self.repeat
            && repeat > MAX_REPEAT
        {
            return Err(Error::invalid(
                "repeat",
                format!("must be {MAX_REPEAT} or fewer"),
            ));
        }

        let mut params = vec![
            ("receptor", self.receptors.join(",")),
            ("message", self.message),
        ];
        if let Some(date) = self.date {
            params.push(("date", date.to_string()));
        }
        if !self.local_ids.is_empty() {
            params.push(("localid", self.local_ids.join(",")));
        }
        if let Some(repeat) = self.repeat {
            params.push(("repeat", repeat.to_string()));
        }
        if let Some(tag) = self.tag {
            params.push(("tag", tag));
        }
        Ok((self.client, params))
    }
}

impl<'a> IntoFuture for TextToSpeechBuilder<'a> {
    type Output = Result<Vec<Message>>;
    type IntoFuture = BoxFuture<'a, Result<Vec<Message>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let (client, params) = self.into_params()?;
            client
                .call_list(Verb::Post, MODULE, "maketts", params)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_is_capped_at_five() {
        let c = Kavenegar::new("test-key");
        assert!(
            c.call()
                .text_to_speech(["0912"], "hello")
                .repeat(5)
                .into_params()
                .is_ok()
        );
        let err = c
            .call()
            .text_to_speech(["0912"], "hello")
            .repeat(6)
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "repeat",
                ..
            }
        ));
    }

    #[test]
    fn tts_builds_the_expected_form() {
        let c = Kavenegar::new("test-key");
        let (_, params) = c
            .call()
            .text_to_speech(["0912", "0913"], "سفارش شما ارسال شد")
            .repeat(2)
            .tag("delivery")
            .into_params()
            .unwrap();

        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("receptor"), Some("0912,0913"));
        assert_eq!(get("repeat"), Some("2"));
        assert_eq!(get("tag"), Some("delivery"));
    }
}
