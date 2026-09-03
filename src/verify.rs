//! One-time passwords and notification templates.
//!
//! The lookup endpoint sends a message built from a template you registered in
//! the Kavenegar panel. It reaches recipients who have opted out of bulk
//! messaging, and it does not require a dedicated line — which is why it is the
//! right call for sign-in codes and transactional alerts.

use std::future::IntoFuture;

use serde::{Deserialize, Serialize};

use crate::client::{BoxFuture, Kavenegar};
use crate::error::{Error, Result};
use crate::models::Message;
use crate::page::Page;

const MODULE: &str = "verify";

/// A validated request: the client to send it with, the endpoint, and the form.
type TemplateRequest<'a> = (&'a Kavenegar, &'static str, Vec<(&'static str, String)>);

/// Optionally redirects a template call at a sub-account.
///
/// Every `verify` template endpoint accepts these, so resellers can manage a
/// customer's templates without holding a separate client per customer.
#[derive(Debug, Clone, Default)]
struct SubAccount {
    api_key: Option<String>,
    local_id: Option<String>,
}

impl SubAccount {
    fn push_into(self, params: &mut Vec<(&'static str, String)>) {
        if let Some(api_key) = self.api_key {
            params.push(("apiKey", api_key));
        }
        if let Some(local_id) = self.local_id {
            params.push(("localId", local_id));
        }
    }
}

/// Generates the sub-account selectors shared by every template builder.
macro_rules! sub_account_methods {
    () => {
        /// Targets a sub-account's templates, named by that account's API key.
        pub fn sub_account_api_key(mut self, api_key: impl Into<String>) -> Self {
            self.sub.api_key = Some(api_key.into());
            self
        }

        /// Targets a sub-account's templates, named by its local id.
        pub fn sub_account_local_id(mut self, local_id: impl Into<String>) -> Self {
            self.sub.local_id = Some(local_id.into());
            self
        }
    };
}

/// Entry point for the `verify` endpoints, from [`Kavenegar::verify`].
#[derive(Debug, Clone, Copy)]
pub struct VerifyService<'a> {
    client: &'a Kavenegar,
}

impl<'a> VerifyService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Sends a templated message, substituting `%token` with `token`.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let sent = client
    ///     .verify()
    ///     .lookup("09121234567", "verify-login", "48391")
    ///     .await?;
    /// println!("status: {}", sent.status);
    /// # Ok(())
    /// # }
    /// ```
    pub fn lookup(
        self,
        receptor: impl Into<String>,
        template: impl Into<String>,
        token: impl Into<String>,
    ) -> LookupBuilder<'a> {
        LookupBuilder {
            client: self.client,
            receptor: receptor.into(),
            template: template.into(),
            token: token.into(),
            token2: None,
            token3: None,
            token10: None,
            token20: None,
            channel: None,
            tag: None,
        }
    }

    /// Lists the verification templates registered on the account.
    ///
    /// Results are paginated at 200 templates per page.
    pub fn templates(self) -> TemplateListBuilder<'a> {
        TemplateListBuilder {
            client: self.client,
            page: None,
            sub: SubAccount::default(),
        }
    }

    /// Fetches one template by its identifier.
    pub fn template(self, id: i64) -> GetTemplateBuilder<'a> {
        GetTemplateBuilder {
            client: self.client,
            id,
            sub: SubAccount::default(),
        }
    }

    /// Creates a template.
    ///
    /// The name must be English, without spaces or underscores, and the body
    /// must contain `%token`. A new template starts in review; check
    /// [`Template::approval_status`] before relying on it.
    ///
    /// ```no_run
    /// # use kavenegar::{SendMethod, SourceType};
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let template = client
    ///     .verify()
    ///     .add_template("LoginOtp", SourceType::Website, SendMethod::Sms)
    ///     .text_message("کد ورود شما %token است")
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_template(
        self,
        name: impl Into<String>,
        source_type: SourceType,
        send_method: SendMethod,
    ) -> TemplateBuilder<'a> {
        TemplateBuilder::new(
            self.client,
            "addtemplate",
            None,
            name.into(),
            source_type,
            send_method,
        )
    }

    /// Replaces an existing template's settings.
    ///
    /// Every field is sent as given, so pass the values you want the template
    /// to end up with, not just the ones that changed.
    pub fn update_template(
        self,
        id: i64,
        name: impl Into<String>,
        source_type: SourceType,
        send_method: SendMethod,
    ) -> TemplateBuilder<'a> {
        TemplateBuilder::new(
            self.client,
            "updatetemplate",
            Some(id),
            name.into(),
            source_type,
            send_method,
        )
    }

    /// Copies an approved template under a new name.
    ///
    /// Identify the source by id with [`CloneTemplateBuilder::from_id`], or by
    /// name with [`CloneTemplateBuilder::from_name`]; the id wins if both are
    /// given.
    pub fn clone_template(self, new_name: impl Into<String>) -> CloneTemplateBuilder<'a> {
        CloneTemplateBuilder {
            client: self.client,
            new_name: new_name.into(),
            source_id: None,
            source_name: None,
            sub: SubAccount::default(),
        }
    }

    /// Deletes a template by its identifier.
    pub fn delete_template(self, id: i64) -> DeleteTemplateBuilder<'a> {
        DeleteTemplateBuilder {
            client: self.client,
            id,
            sub: SubAccount::default(),
        }
    }
}

/// Builds a call to `verify/templatelist`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct TemplateListBuilder<'a> {
    client: &'a Kavenegar,
    page: Option<u32>,
    sub: SubAccount,
}

impl<'a> TemplateListBuilder<'a> {
    /// Selects a page, counting from 1. Defaults to the first page.
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    sub_account_methods!();
}

impl<'a> IntoFuture for TemplateListBuilder<'a> {
    type Output = Result<Page<Template>>;
    type IntoFuture = BoxFuture<'a, Result<Page<Template>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut params = Vec::new();
            if let Some(page) = self.page {
                params.push(("page", page.to_string()));
            }
            self.sub.push_into(&mut params);
            self.client.call_page(MODULE, "templatelist", params).await
        })
    }
}

/// Builds a call to `verify/gettemplate`. Await it to fetch the template.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct GetTemplateBuilder<'a> {
    client: &'a Kavenegar,
    id: i64,
    sub: SubAccount,
}

impl<'a> GetTemplateBuilder<'a> {
    sub_account_methods!();
}

impl<'a> IntoFuture for GetTemplateBuilder<'a> {
    type Output = Result<Template>;
    type IntoFuture = BoxFuture<'a, Result<Template>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut params = vec![("id", self.id.to_string())];
            self.sub.push_into(&mut params);
            self.client.call_one(MODULE, "gettemplate", params).await
        })
    }
}

/// Builds a call to `verify/deletetemplate`. Await it to delete the template.
#[derive(Debug)]
#[must_use = "the template is only deleted when the builder is awaited"]
pub struct DeleteTemplateBuilder<'a> {
    client: &'a Kavenegar,
    id: i64,
    sub: SubAccount,
}

impl<'a> DeleteTemplateBuilder<'a> {
    sub_account_methods!();
}

impl<'a> IntoFuture for DeleteTemplateBuilder<'a> {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'a, Result<()>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut params = vec![("id", self.id.to_string())];
            self.sub.push_into(&mut params);
            self.client
                .call_discard(MODULE, "deletetemplate", params)
                .await
        })
    }
}

/// Builds a call to `verify/lookup`. Await it to send the message.
///
/// A template may carry up to five substitutions. `%token` is required; the
/// rest are optional and each has its own length limit, enforced by Kavenegar:
/// `%token2` and `%token3` are short, `%token10` and `%token20` allow longer
/// values and permit spaces.
#[derive(Debug)]
#[must_use = "the message is only sent when the builder is awaited"]
pub struct LookupBuilder<'a> {
    client: &'a Kavenegar,
    receptor: String,
    template: String,
    token: String,
    token2: Option<String>,
    token3: Option<String>,
    token10: Option<String>,
    token20: Option<String>,
    channel: Option<VerifyChannel>,
    tag: Option<String>,
}

impl<'a> LookupBuilder<'a> {
    /// Substitutes `%token2`.
    pub fn token2(mut self, value: impl Into<String>) -> Self {
        self.token2 = Some(value.into());
        self
    }

    /// Substitutes `%token3`.
    pub fn token3(mut self, value: impl Into<String>) -> Self {
        self.token3 = Some(value.into());
        self
    }

    /// Substitutes `%token10`, which accepts a longer value with spaces.
    pub fn token10(mut self, value: impl Into<String>) -> Self {
        self.token10 = Some(value.into());
        self
    }

    /// Substitutes `%token20`, which accepts the longest value.
    pub fn token20(mut self, value: impl Into<String>) -> Self {
        self.token20 = Some(value.into());
        self
    }

    /// Delivers the token over a voice call instead of an SMS.
    ///
    /// A voice call can only read out a numeric token; Kavenegar rejects
    /// anything else with [`ApiStatus::TokenNotDialable`].
    ///
    /// [`ApiStatus::TokenNotDialable`]: crate::ApiStatus::TokenNotDialable
    pub fn channel(mut self, channel: VerifyChannel) -> Self {
        self.channel = Some(channel);
        self
    }

    /// Tags the message, for grouping in reports.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    fn into_params(self) -> Result<(&'a Kavenegar, Vec<(&'static str, String)>)> {
        if self.receptor.trim().is_empty() {
            return Err(Error::invalid("receptor", "must not be empty"));
        }
        if self.template.trim().is_empty() {
            return Err(Error::invalid("template", "must not be empty"));
        }
        if self.token.trim().is_empty() {
            return Err(Error::invalid("token", "must not be empty"));
        }
        // Kavenegar splits parameters on whitespace, so a token containing a
        // space silently truncates the message. Catch it before sending.
        if self.token.contains(char::is_whitespace) {
            return Err(Error::invalid(
                "token",
                "must not contain whitespace; use token10 or token20 for values with spaces",
            ));
        }

        let mut params = vec![
            ("receptor", self.receptor),
            ("template", self.template),
            ("token", self.token),
        ];
        if let Some(v) = self.token2 {
            params.push(("token2", v));
        }
        if let Some(v) = self.token3 {
            params.push(("token3", v));
        }
        if let Some(v) = self.token10 {
            params.push(("token10", v));
        }
        if let Some(v) = self.token20 {
            params.push(("token20", v));
        }
        if let Some(channel) = self.channel {
            params.push(("type", channel.as_str().to_owned()));
        }
        if let Some(tag) = self.tag {
            params.push(("tag", tag));
        }
        Ok((self.client, params))
    }
}

impl<'a> IntoFuture for LookupBuilder<'a> {
    type Output = Result<Message>;
    type IntoFuture = BoxFuture<'a, Result<Message>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let (client, params) = self.into_params()?;
            client.call_one(MODULE, "lookup", params).await
        })
    }
}

/// How a verification token should reach the recipient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum VerifyChannel {
    /// Deliver as an SMS. The API default.
    #[default]
    Sms,
    /// Read the token out over a voice call.
    Call,
}

impl VerifyChannel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            VerifyChannel::Sms => "sms",
            VerifyChannel::Call => "call",
        }
    }
}

/// Where a template is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum SourceType {
    /// A website.
    #[default]
    Website,
    /// A mobile application.
    MobileApp,
}

impl SourceType {
    pub(crate) fn code(self) -> &'static str {
        match self {
            SourceType::Website => "0",
            SourceType::MobileApp => "1",
        }
    }
}

/// The channel a template sends over first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum SendMethod {
    /// Deliver as an SMS.
    #[default]
    Sms,
    /// Read the token out over a voice call.
    Call,
}

impl SendMethod {
    pub(crate) fn code(self) -> &'static str {
        match self {
            SendMethod::Sms => "1",
            SendMethod::Call => "2",
        }
    }
}

/// The channel tried when the primary one does not get through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum FallbackMethod {
    /// Let Kavenegar decide.
    SystemDefault,
    /// Fall back to an SMS.
    Sms,
    /// Fall back to a voice call.
    Call,
    /// Do not fall back. The API default.
    #[default]
    Disabled,
}

impl FallbackMethod {
    pub(crate) fn code(self) -> &'static str {
        match self {
            FallbackMethod::SystemDefault => "0",
            FallbackMethod::Sms => "1",
            FallbackMethod::Call => "2",
            FallbackMethod::Disabled => "3",
        }
    }

    /// Whether this fallback actually sends anything.
    pub fn is_enabled(self) -> bool {
        self != FallbackMethod::Disabled
    }
}

/// Builds a call to `verify/addtemplate` or `verify/updatetemplate`.
///
/// Await it to create or replace the template.
#[derive(Debug)]
#[must_use = "the template is only saved when the builder is awaited"]
pub struct TemplateBuilder<'a> {
    client: &'a Kavenegar,
    method: &'static str,
    id: Option<i64>,
    name: String,
    source_type: SourceType,
    send_method: SendMethod,
    fallback_method: Option<FallbackMethod>,
    primary_line_number: Option<String>,
    secondary_line_number: Option<String>,
    switch_ttl: Option<u8>,
    source_url: Option<String>,
    source_name: Option<String>,
    text_message: Option<String>,
    voice_message: Option<String>,
    sub: SubAccount,
}

impl<'a> TemplateBuilder<'a> {
    pub(crate) fn new(
        client: &'a Kavenegar,
        method: &'static str,
        id: Option<i64>,
        name: String,
        source_type: SourceType,
        send_method: SendMethod,
    ) -> Self {
        Self {
            client,
            method,
            id,
            name,
            source_type,
            send_method,
            fallback_method: None,
            primary_line_number: None,
            secondary_line_number: None,
            switch_ttl: None,
            source_url: None,
            source_name: None,
            text_message: None,
            voice_message: None,
            sub: SubAccount::default(),
        }
    }

    /// Sets the SMS body. Must contain `%token`.
    pub fn text_message(mut self, message: impl Into<String>) -> Self {
        self.text_message = Some(message.into());
        self
    }

    /// Sets the text read aloud on a voice call. Must contain `%token`.
    pub fn voice_message(mut self, message: impl Into<String>) -> Self {
        self.voice_message = Some(message.into());
        self
    }

    /// Chooses the channel to fall back to. Defaults to no fallback.
    pub fn fallback_method(mut self, method: FallbackMethod) -> Self {
        self.fallback_method = Some(method);
        self
    }

    /// Sets the line the template sends from.
    pub fn primary_line_number(mut self, line: impl Into<String>) -> Self {
        self.primary_line_number = Some(line.into());
        self
    }

    /// Sets the line used when the primary one is unavailable.
    pub fn secondary_line_number(mut self, line: impl Into<String>) -> Self {
        self.secondary_line_number = Some(line.into());
        self
    }

    /// Sets how long to wait, 1 to 5 minutes, before trying the fallback.
    pub fn switch_ttl(mut self, minutes: u8) -> Self {
        self.switch_ttl = Some(minutes);
        self
    }

    /// Records the website or app the template belongs to.
    pub fn source_url(mut self, url: impl Into<String>) -> Self {
        self.source_url = Some(url.into());
        self
    }

    /// Records the name of the website or app.
    pub fn source_name(mut self, name: impl Into<String>) -> Self {
        self.source_name = Some(name.into());
        self
    }

    sub_account_methods!();

    fn into_params(self) -> Result<TemplateRequest<'a>> {
        // Kavenegar rejects these outright, so catch them here rather than
        // spending a request to be told.
        if self.name.trim().is_empty() {
            return Err(Error::invalid("name", "must not be empty"));
        }
        if !self.name.is_ascii() {
            return Err(Error::invalid("name", "must be English"));
        }
        if self.name.contains(char::is_whitespace) || self.name.contains('_') {
            return Err(Error::invalid(
                "name",
                "must not contain spaces or underscores",
            ));
        }
        if let Some(ttl) = self.switch_ttl
            && !(1..=5).contains(&ttl)
        {
            return Err(Error::invalid("switch_ttl", "must be between 1 and 5"));
        }

        let fallback = self.fallback_method.unwrap_or_default();
        let sms_used = self.send_method == SendMethod::Sms || fallback == FallbackMethod::Sms;
        let call_used = self.send_method == SendMethod::Call || fallback == FallbackMethod::Call;

        if sms_used {
            match self.text_message.as_deref() {
                None => return Err(Error::invalid("text_message", "required for SMS delivery")),
                Some(body) if !body.contains("%token") => {
                    return Err(Error::invalid("text_message", "must contain `%token`"));
                }
                Some(_) => {}
            }
        }
        if call_used {
            match self.voice_message.as_deref() {
                None => {
                    return Err(Error::invalid(
                        "voice_message",
                        "required for voice delivery",
                    ));
                }
                Some(body) if !(body.contains("%token") || body.contains("$token")) => {
                    return Err(Error::invalid(
                        "voice_message",
                        "must contain `%token` or `$token`",
                    ));
                }
                Some(_) => {}
            }
        }

        let mut params = vec![
            ("name", self.name),
            ("sourceType", self.source_type.code().to_owned()),
            ("sendMethod", self.send_method.code().to_owned()),
        ];
        if let Some(id) = self.id {
            params.push(("templateId", id.to_string()));
        }
        if let Some(fallback) = self.fallback_method {
            params.push(("fallBackMethod", fallback.code().to_owned()));
        }
        if let Some(v) = self.primary_line_number {
            params.push(("primaryLineNumber", v));
        }
        if let Some(v) = self.secondary_line_number {
            params.push(("secondaryLineNumber", v));
        }
        if let Some(v) = self.switch_ttl {
            params.push(("switchTTL", v.to_string()));
        }
        if let Some(v) = self.source_url {
            params.push(("sourceUrl", v));
        }
        if let Some(v) = self.source_name {
            params.push(("sourceName", v));
        }
        if let Some(v) = self.text_message {
            params.push(("textMessage", v));
        }
        if let Some(v) = self.voice_message {
            params.push(("voiceMessage", v));
        }
        self.sub.push_into(&mut params);
        Ok((self.client, self.method, params))
    }
}

impl<'a> IntoFuture for TemplateBuilder<'a> {
    type Output = Result<Template>;
    type IntoFuture = BoxFuture<'a, Result<Template>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let (client, method, params) = self.into_params()?;
            client.call_one(MODULE, method, params).await
        })
    }
}

/// Builds a call to `verify/clonetemplate`. Await it to copy the template.
#[derive(Debug)]
#[must_use = "the template is only copied when the builder is awaited"]
pub struct CloneTemplateBuilder<'a> {
    client: &'a Kavenegar,
    new_name: String,
    source_id: Option<i64>,
    source_name: Option<String>,
    sub: SubAccount,
}

impl<'a> CloneTemplateBuilder<'a> {
    /// Identifies the source template by its id. Takes priority over a name.
    pub fn from_id(mut self, id: i64) -> Self {
        self.source_id = Some(id);
        self
    }

    /// Identifies the source template by its name.
    pub fn from_name(mut self, name: impl Into<String>) -> Self {
        self.source_name = Some(name.into());
        self
    }

    sub_account_methods!();

    fn into_params(self) -> Result<(&'a Kavenegar, Vec<(&'static str, String)>)> {
        if self.new_name.trim().is_empty() {
            return Err(Error::invalid("new_name", "must not be empty"));
        }
        if self.source_id.is_none() && self.source_name.is_none() {
            return Err(Error::invalid(
                "source",
                "identify the source with from_id or from_name",
            ));
        }

        let mut params = vec![("newTemplateName", self.new_name)];
        if let Some(id) = self.source_id {
            params.push(("sourceTemplateId", id.to_string()));
        }
        if let Some(name) = self.source_name {
            params.push(("sourceTemplateName", name));
        }
        self.sub.push_into(&mut params);
        Ok((self.client, params))
    }
}

impl<'a> IntoFuture for CloneTemplateBuilder<'a> {
    type Output = Result<Template>;
    type IntoFuture = BoxFuture<'a, Result<Template>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let (client, params) = self.into_params()?;
            client.call_one(MODULE, "clonetemplate", params).await
        })
    }
}

/// A verification template registered on the account.
///
/// Kavenegar is inconsistent about the casing of these field names between
/// endpoints, so each field accepts the documented spellings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Template {
    /// The template's numeric identifier.
    #[serde(default, alias = "templateid")]
    pub id: i64,
    /// The name passed to [`VerifyService::lookup`].
    #[serde(default)]
    pub name: String,
    /// The SMS body, including its `%token` placeholders.
    #[serde(default, alias = "smsMessage", alias = "textmessage")]
    pub sms_message: String,
    /// The text read aloud when the template is delivered by voice call.
    #[serde(default, alias = "callMessage", alias = "voicemessage")]
    pub call_message: String,
    /// The line the template sends from.
    #[serde(default, alias = "primaryLineNumber")]
    pub primary_line_number: String,
    /// The line used when the primary one is unavailable.
    #[serde(default, alias = "secondaryLineNumber")]
    pub secondary_line_number: String,
    /// Which channel is tried first: `SMS` or `Call`.
    #[serde(default, alias = "sendPriority")]
    pub send_priority: String,
    /// How long to wait, in minutes, before falling back to the other channel.
    #[serde(default, alias = "switchTTL", alias = "switchttL")]
    pub switch_ttl: i64,
    /// Where the template sits in the approval process.
    #[serde(default, alias = "approvalStatus")]
    pub approval_status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Kavenegar {
        Kavenegar::new("test-key")
    }

    #[test]
    fn lookup_requires_its_three_core_parameters() {
        let c = client();
        for (receptor, template, token, expected) in [
            ("", "t", "1", "receptor"),
            ("0912", "", "1", "template"),
            ("0912", "t", "  ", "token"),
        ] {
            let err = c
                .verify()
                .lookup(receptor, template, token)
                .into_params()
                .unwrap_err();
            match err {
                Error::InvalidParameter { parameter, .. } => assert_eq!(parameter, expected),
                other => panic!("expected an invalid-parameter error, got {other:?}"),
            }
        }
    }

    #[test]
    fn lookup_rejects_a_token_with_whitespace() {
        let c = client();
        let err = c
            .verify()
            .lookup("0912", "t", "12 34")
            .into_params()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidParameter {
                parameter: "token",
                ..
            }
        ));
    }

    #[test]
    fn lookup_builds_the_expected_form() {
        let c = client();
        let (_, params) = c
            .verify()
            .lookup("09121234567", "verify-login", "48391")
            .token10("Ali Rezaei")
            .channel(VerifyChannel::Call)
            .into_params()
            .unwrap();

        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("receptor"), Some("09121234567"));
        assert_eq!(get("template"), Some("verify-login"));
        assert_eq!(get("token"), Some("48391"));
        assert_eq!(get("token10"), Some("Ali Rezaei"));
        assert_eq!(get("type"), Some("call"));
        assert_eq!(get("token2"), None);
    }
}
