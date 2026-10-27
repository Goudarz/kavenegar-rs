//! Credit, expiry and account settings.

use std::future::IntoFuture;

use serde::{Deserialize, Serialize};

use crate::client::{BoxFuture, Kavenegar, Verb};
use crate::de::{lenient_i64, lenient_string};
use crate::error::Result;

const MODULE: &str = "account";

/// Entry point for the `account` endpoints, from [`Kavenegar::account`].
#[derive(Debug, Clone, Copy)]
pub struct AccountService<'a> {
    client: &'a Kavenegar,
}

impl<'a> AccountService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Reads the remaining credit, expiry date and account tier.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let info = client.account().info().await?;
    /// println!("{} Rials remaining", info.remaining_credit);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Api`](crate::Error::Api) when Kavenegar refuses the request, or
    /// another [`Error`](crate::Error) if it cannot be sent or decoded.
    pub async fn info(self) -> Result<AccountInfo> {
        self.client
            .call_one(Verb::Post, MODULE, "info", Vec::new())
            .await
    }

    /// Reads the current account settings, changing nothing.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Api`](crate::Error::Api) when Kavenegar refuses the request, or
    /// another [`Error`](crate::Error) if it cannot be sent or decoded.
    pub async fn config(self) -> Result<AccountConfig> {
        // Documented as a parameterless GET.
        self.client
            .call_one(Verb::Get, MODULE, "config", Vec::new())
            .await
    }

    /// Updates account settings. Only the fields you set are sent, and the
    /// full settings are returned as they stand afterwards.
    ///
    /// ```no_run
    /// # use kavenegar::Toggle;
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let updated = client
    ///     .account()
    ///     .update_config()
    ///     .default_sender("10004346")
    ///     .daily_report(Toggle::Enabled)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_config(self) -> UpdateConfigBuilder<'a> {
        UpdateConfigBuilder {
            client: self.client,
            params: Vec::new(),
        }
    }
}

/// The account state returned by `account/info`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct AccountInfo {
    /// Remaining credit in Rials.
    #[serde(rename = "remaincredit", default, deserialize_with = "lenient_i64")]
    pub remaining_credit: i64,
    /// Account expiry, as a Unix timestamp. Kavenegar describes this as a
    /// security measure for sub-accounts, not a deadline to act on.
    #[serde(rename = "expiredate", default, deserialize_with = "lenient_i64")]
    pub expire_date: i64,
    /// Account tier: `master` or `child`.
    #[serde(rename = "type", default, deserialize_with = "lenient_string")]
    pub account_type: String,
}

/// The settings returned and accepted by `account/config`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct AccountConfig {
    /// API logging mode: `enabled`, `disabled` or `justfaults`.
    #[serde(rename = "apilogs", default, deserialize_with = "lenient_string")]
    pub api_logs: String,
    /// Daily report mode.
    #[serde(rename = "dailyreport", default, deserialize_with = "lenient_string")]
    pub daily_report: String,
    /// Debug mode. While enabled, sends are accepted but never delivered.
    #[serde(rename = "debugmode", default, deserialize_with = "lenient_string")]
    pub debug_mode: String,
    /// The line used when a send omits a sender.
    #[serde(rename = "defaultsender", default, deserialize_with = "lenient_string")]
    pub default_sender: String,
    /// Credit threshold, in Rials, that triggers an alert.
    #[serde(rename = "mincreditalarm", default, deserialize_with = "lenient_i64")]
    pub min_credit_alarm: i64,
    /// Whether undelivered messages are resent automatically. The API has
    /// been seen to report this as `true` as well as `enabled`.
    #[serde(rename = "resendfailed", default, deserialize_with = "lenient_string")]
    pub resend_failed: String,
}

/// A toggle accepted by several `account/config` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Toggle {
    /// Turn the feature on.
    Enabled,
    /// Turn the feature off.
    Disabled,
    /// Report only failures. Not valid for every field.
    JustFaults,
}

impl Toggle {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Toggle::Enabled => "enabled",
            Toggle::Disabled => "disabled",
            Toggle::JustFaults => "justfaults",
        }
    }
}

/// Builds a call to `account/config`. Await it to apply the changes.
#[derive(Debug)]
#[must_use = "the settings are only applied when the builder is awaited"]
pub struct UpdateConfigBuilder<'a> {
    client: &'a Kavenegar,
    params: Vec<(&'static str, String)>,
}

impl<'a> UpdateConfigBuilder<'a> {
    /// Controls whether API calls are logged.
    pub fn api_logs(mut self, value: Toggle) -> Self {
        self.params.push(("apilogs", value.as_str().to_owned()));
        self
    }

    /// Controls the daily usage report.
    pub fn daily_report(mut self, value: Toggle) -> Self {
        self.params.push(("dailyreport", value.as_str().to_owned()));
        self
    }

    /// Controls debug mode. While enabled, sends are accepted and charged
    /// nothing, but no message is delivered — useful for testing your code.
    pub fn debug_mode(mut self, value: Toggle) -> Self {
        self.params.push(("debugmode", value.as_str().to_owned()));
        self
    }

    /// Sets the line used when a send omits a sender.
    pub fn default_sender(mut self, sender: impl Into<String>) -> Self {
        self.params.push(("defaultsender", sender.into()));
        self
    }

    /// Sets the credit threshold, in Rials, that triggers a low-balance alert.
    pub fn min_credit_alarm(mut self, rials: i64) -> Self {
        self.params.push(("mincreditalarm", rials.to_string()));
        self
    }

    /// Controls whether failed messages are resent automatically.
    pub fn resend_failed(mut self, value: Toggle) -> Self {
        self.params
            .push(("resendfailed", value.as_str().to_owned()));
        self
    }
}

impl<'a> IntoFuture for UpdateConfigBuilder<'a> {
    type Output = Result<AccountConfig>;
    type IntoFuture = BoxFuture<'a, Result<AccountConfig>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            self.client
                .call_one(Verb::Post, MODULE, "config", self.params)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_config_sends_only_what_was_set() {
        let c = Kavenegar::new("test-key");
        let builder = c
            .account()
            .update_config()
            .default_sender("10004346")
            .daily_report(Toggle::JustFaults);

        assert_eq!(builder.params.len(), 2);
        assert_eq!(builder.params[0], ("defaultsender", "10004346".to_owned()));
        assert_eq!(builder.params[1], ("dailyreport", "justfaults".to_owned()));
    }

    #[test]
    fn config_decodes_the_documented_payload() {
        // The documented example sends `mincreditalarm` as a string.
        let config: AccountConfig = serde_json::from_str(
            r#"{"apilogs":"justfaults","dailyreport":"disabled","debugmode":"enabled",
                "defaultsender":"10004535","mincreditalarm":"50000","resendfailed":"true"}"#,
        )
        .unwrap();
        assert_eq!(config.min_credit_alarm, 50000);
        assert_eq!(config.default_sender, "10004535");
        assert_eq!(config.resend_failed, "true");
    }

    #[test]
    fn an_untouched_update_sends_nothing() {
        let c = Kavenegar::new("test-key");
        assert!(c.account().update_config().params.is_empty());
    }
}
