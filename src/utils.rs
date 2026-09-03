//! Utility endpoints.

use serde::{Deserialize, Serialize};

use crate::client::Kavenegar;
use crate::error::Result;

const MODULE: &str = "utils";

/// Entry point for the `utils` endpoints, from [`Kavenegar::utils`].
#[derive(Debug, Clone, Copy)]
pub struct UtilsService<'a> {
    client: &'a Kavenegar,
}

impl<'a> UtilsService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Reads Kavenegar's own clock.
    ///
    /// Every timestamp in this API is a Unix time from the Kavenegar server,
    /// so this is the reference to schedule against when your own clock might
    /// drift. It is also the cheapest way to check that an API key works.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
    /// let now = client.utils().server_time().await?;
    /// println!("server unix time: {}", now.unix_time);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn server_time(self) -> Result<ServerTime> {
        self.client.call_one(MODULE, "getdate", Vec::new()).await
    }
}

/// The Kavenegar server's current date and time.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct ServerTime {
    /// The time formatted by the server, in Persian.
    #[serde(default)]
    pub datetime: String,
    /// Year.
    #[serde(default)]
    pub year: i64,
    /// Month.
    #[serde(default)]
    pub month: i64,
    /// Day.
    #[serde(default)]
    pub day: i64,
    /// Hour, on a 24-hour clock.
    #[serde(default)]
    pub hour: i64,
    /// Minute.
    #[serde(default)]
    pub minute: i64,
    /// Second.
    #[serde(default)]
    pub second: i64,
    /// The same instant as a Unix timestamp in seconds.
    #[serde(rename = "unixtime", default)]
    pub unix_time: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_time_decodes_the_documented_payload() {
        let json = r#"{"datetime":"02/06/2012 01:53:46 ب.ظ","year":2012,"month":1,
                       "day":26,"hour":13,"minute":53,"second":46,"unixtime":1338645226}"#;
        let time: ServerTime = serde_json::from_str(json).unwrap();

        assert_eq!(time.unix_time, 1338645226);
        assert_eq!(time.year, 2012);
        assert_eq!(time.hour, 13);
    }
}
