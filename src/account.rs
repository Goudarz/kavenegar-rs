//! Credit, expiry and account settings.

use std::future::IntoFuture;

use serde::{Deserialize, Serialize};

use crate::client::{BoxFuture, Kavenegar};
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
    pub async fn info(self) -> Result<AccountInfo> {
        self.client.call_one(MODULE, "info", Vec::new()).await
    }

    /// Reads the current account settings, changing nothing.
    pub async fn config(self) -> Result<AccountConfig> {
        self.client.call_one(MODULE, "config", Vec::new()).await
    }

    /// Updates account settings. Only the fields you set are sent.
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
    #[serde(rename = "remaincredit", default)]
    pub remaining_credit: i64,
    /// Account expiry, as a Unix timestamp.
    #[serde(rename = "expiredate", default)]
    pub expire_date: i64,
    /// Account tier, for example `master`.
    #[serde(rename = "type", default)]
    pub account_type: String,
}

/// The settings returned and accepted by `account/config`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct AccountConfig {
    /// API logging mode.
    #[serde(rename = "apilogs", default)]
    pub api_logs: String,
    /// Daily report mode.
    #[serde(rename = "dailyreport", default)]
    pub daily_report: String,
    /// Debug mode.
    #[serde(rename = "debugmode", default)]
    pub debug_mode: String,
    /// The line used when a send omits a sender.
    #[serde(rename = "defaultsender", default)]
    pub default_sender: String,
    /// Credit threshold, in Rials, that triggers an alert.
    #[serde(rename = "mincreditalarm", default)]
    pub min_credit_alarm: i64,
    /// Whether failed messages are resent automatically.
    #[serde(rename = "resendfailed", default)]
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

    /// Controls debug mode.
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
        Box::pin(async move { self.client.call_one(MODULE, "config", self.params).await })
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
    fn an_untouched_update_sends_nothing() {
        let c = Kavenegar::new("test-key");
        assert!(c.account().update_config().params.is_empty());
    }
}
