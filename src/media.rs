//! Media files usable by messenger lines.
//!
//! Uploaded files are usable only by domestic messenger lines, and Kavenegar
//! deletes any file that goes 30 consecutive days without use.
//!
//! Upload limits: images (`image/jpeg`, `image/gif`) up to 10 MB with no
//! duration limit; video (`video/mp4`) up to 20 MB and 60 seconds.

use std::future::IntoFuture;

use serde::{Deserialize, Deserializer, Serialize};

use crate::client::{BoxFuture, Kavenegar};
use crate::error::{Error, Result};
use crate::page::Page;

const MODULE: &str = "media";

/// Entry point for the `media` endpoints, from [`Kavenegar::media`].
#[derive(Debug, Clone, Copy)]
pub struct MediaService<'a> {
    client: &'a Kavenegar,
}

impl<'a> MediaService<'a> {
    pub(crate) fn new(client: &'a Kavenegar) -> Self {
        Self { client }
    }

    /// Uploads a file.
    ///
    /// This crate does not read the file for you, so you stay in charge of
    /// how it is loaded — with `tokio::fs::read` in async code, or
    /// `std::fs::read` elsewhere.
    ///
    /// ```no_run
    /// # async fn run(client: kavenegar::Kavenegar) -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = tokio::fs::read("sample.jpg").await?;
    /// let media = client
    ///     .media()
    ///     .upload("sample.jpg", bytes)
    ///     .mime("image/jpeg")
    ///     .await?;
    /// println!("uploaded as {}", media.id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn upload(
        self,
        file_name: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> UploadBuilder<'a> {
        UploadBuilder {
            client: self.client,
            file_name: file_name.into(),
            bytes: bytes.into(),
            mime: None,
        }
    }

    /// Lists uploaded media, 50 per page by default.
    pub fn list(self) -> MediaListBuilder<'a> {
        MediaListBuilder {
            client: self.client,
            page: None,
            size: None,
        }
    }

    /// Fetches one media file's details by its identifier.
    pub async fn get(self, id: impl Into<String>) -> Result<Media> {
        let id = non_empty_id(id)?;
        self.client.call_one(MODULE, "get", vec![("id", id)]).await
    }

    /// Deletes a media file permanently.
    pub async fn delete(self, id: impl Into<String>) -> Result<MediaDeletion> {
        let id = non_empty_id(id)?;
        self.client
            .call_one(MODULE, "delete", vec![("id", id)])
            .await
    }
}

/// Builds a call to `media/upload`. Await it to send the file.
#[derive(Debug)]
#[must_use = "the file is only uploaded when the builder is awaited"]
pub struct UploadBuilder<'a> {
    client: &'a Kavenegar,
    file_name: String,
    bytes: Vec<u8>,
    mime: Option<String>,
}

impl<'a> UploadBuilder<'a> {
    /// Declares the file's MIME type.
    ///
    /// Without it, the type is guessed from the file extension.
    pub fn mime(mut self, mime: impl Into<String>) -> Self {
        self.mime = Some(mime.into());
        self
    }
}

impl<'a> IntoFuture for UploadBuilder<'a> {
    type Output = Result<Media>;
    type IntoFuture = BoxFuture<'a, Result<Media>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            if self.file_name.trim().is_empty() {
                return Err(Error::invalid("file_name", "must not be empty"));
            }
            if self.bytes.is_empty() {
                return Err(Error::invalid("bytes", "must not be empty"));
            }

            let mime = self
                .mime
                .unwrap_or_else(|| guess_mime(&self.file_name).to_owned());

            let part = reqwest::multipart::Part::bytes(self.bytes)
                .file_name(self.file_name)
                .mime_str(&mime)
                .map_err(|_| {
                    Error::invalid("mime", format!("`{mime}` is not a valid MIME type"))
                })?;

            self.client
                .call_upload(MODULE, "upload", "File", part)
                .await
        })
    }
}

/// Builds a call to `media/list`. Await it to run the query.
#[derive(Debug)]
#[must_use = "the query only runs when the builder is awaited"]
pub struct MediaListBuilder<'a> {
    client: &'a Kavenegar,
    page: Option<u32>,
    size: Option<u32>,
}

impl<'a> MediaListBuilder<'a> {
    /// Selects a page, counting from 1.
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Sets how many records a page holds. Defaults to 50.
    pub fn size(mut self, size: u32) -> Self {
        self.size = Some(size);
        self
    }
}

impl<'a> IntoFuture for MediaListBuilder<'a> {
    type Output = Result<Page<Media>>;
    type IntoFuture = BoxFuture<'a, Result<Page<Media>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut params = Vec::new();
            if let Some(page) = self.page {
                params.push(("page", page.to_string()));
            }
            if let Some(size) = self.size {
                params.push(("size", size.to_string()));
            }

            // `media/list` nests its own pagination inside `entries`, rather
            // than using the `metadata` block the other endpoints use.
            let listing: MediaListing = self.client.call_one(MODULE, "list", params).await?;
            Ok(listing.into_page())
        })
    }
}

/// The `entries` payload of `media/list`.
#[derive(Debug, Deserialize)]
struct MediaListing {
    #[serde(default)]
    page: i64,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    total: i64,
    #[serde(default)]
    list: Vec<Media>,
}

impl MediaListing {
    fn into_page(self) -> Page<Media> {
        // `i64::div_ceil` is still unstable. Values come off the wire, so the
        // addition is checked rather than trusted not to overflow.
        let total_pages = match self.size {
            size if size > 0 => self
                .total
                .checked_add(size - 1)
                .map_or(1, |sum| sum / size)
                .max(1),
            _ => 1,
        };
        Page {
            items: self.list,
            total_count: self.total,
            current_page: self.page.max(1),
            total_pages: total_pages.max(1),
            page_size: self.size,
        }
    }
}

/// An uploaded media file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Media {
    /// The file's unique identifier, a GUID.
    #[serde(default)]
    pub id: String,
    /// The file name.
    #[serde(default)]
    pub name: String,
    /// The file's MIME type.
    #[serde(default)]
    pub mime_type: String,
    /// The file extension, without a leading dot.
    #[serde(default)]
    pub extension: String,
    /// Size in bytes.
    #[serde(default)]
    pub size: i64,
    /// Duration in seconds; `0` for images.
    #[serde(default)]
    pub duration: i64,
    /// Pixel dimensions, such as `1800x600`.
    #[serde(default)]
    pub resolution: String,
    /// How far along the file's processing is.
    #[serde(default)]
    pub status: MediaStatus,
    /// The Persian description of `status`.
    #[serde(default)]
    pub status_desc: String,
    /// The moderation outcome, once the file has been reviewed.
    #[serde(default)]
    pub review: Option<MediaReview>,
}

/// The moderation outcome for an uploaded file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct MediaReview {
    /// `1` when approved, `2` when rejected.
    #[serde(default)]
    pub status: i64,
    /// The Persian description of `status`.
    #[serde(default)]
    pub status_desc: String,
    /// A code explaining the decision.
    #[serde(default)]
    pub reason: i64,
    /// The Persian description of `reason`.
    #[serde(default)]
    pub reason_desc: String,
}

impl MediaReview {
    /// Whether the file cleared moderation.
    pub fn is_approved(&self) -> bool {
        self.status == 1
    }
}

/// How far along a media file's processing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize)]
#[non_exhaustive]
pub enum MediaStatus {
    /// Still being processed.
    #[default]
    Processing,
    /// Processed and usable.
    Ready,
    /// Processing failed.
    Failed,
    /// A state this crate does not recognise.
    Other(i64),
}

impl MediaStatus {
    /// The numeric code as sent by the API.
    pub fn code(self) -> i64 {
        match self {
            MediaStatus::Processing => 0,
            MediaStatus::Ready => 1,
            MediaStatus::Failed => 2,
            MediaStatus::Other(c) => c,
        }
    }

    /// Whether the file is processed and usable.
    pub fn is_ready(self) -> bool {
        self == MediaStatus::Ready
    }
}

impl From<i64> for MediaStatus {
    fn from(code: i64) -> Self {
        match code {
            0 => MediaStatus::Processing,
            1 => MediaStatus::Ready,
            2 => MediaStatus::Failed,
            other => MediaStatus::Other(other),
        }
    }
}

impl<'de> Deserialize<'de> for MediaStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(MediaStatus::from(i64::deserialize(d)?))
    }
}

/// The outcome of deleting a media file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct MediaDeletion {
    /// The identifier that was deleted.
    #[serde(default)]
    pub id: String,
    /// Whether the file is now gone.
    #[serde(default)]
    pub deleted: bool,
}

fn non_empty_id(id: impl Into<String>) -> Result<String> {
    let id = id.into();
    if id.trim().is_empty() {
        return Err(Error::invalid("id", "must not be empty"));
    }
    Ok(id)
}

/// Maps the extensions Kavenegar accepts to their MIME types.
fn guess_mime(file_name: &str) -> &'static str {
    let extension = file_name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_is_guessed_from_the_extension() {
        assert_eq!(guess_mime("sample.jpg"), "image/jpeg");
        assert_eq!(guess_mime("SAMPLE.JPEG"), "image/jpeg");
        assert_eq!(guess_mime("clip.mp4"), "video/mp4");
        assert_eq!(guess_mime("art.gif"), "image/gif");
        assert_eq!(guess_mime("notes.txt"), "application/octet-stream");
    }

    #[test]
    fn media_decodes_the_documented_payload() {
        let json = r#"{
            "id":"3fa85f64-5717-4562-b3fc-2c963f66afa6","name":"sample.jpg",
            "mime_type":"image/jpeg","extension":"jpg","size":2048576,"duration":0,
            "resolution":"1800x600","status":1,"status_desc":"تایید شده",
            "review":{"status":1,"status_desc":"تایید شده","reason":0,"reason_desc":""}
        }"#;
        let media: Media = serde_json::from_str(json).unwrap();

        assert_eq!(media.id, "3fa85f64-5717-4562-b3fc-2c963f66afa6");
        assert_eq!(media.size, 2048576);
        assert!(media.status.is_ready());
        assert!(media.review.unwrap().is_approved());
    }

    #[test]
    fn listing_derives_its_page_count() {
        let listing = MediaListing {
            page: 2,
            size: 50,
            total: 120,
            list: Vec::new(),
        };
        let page = listing.into_page();

        assert_eq!(page.total_pages, 3);
        assert_eq!(page.current_page, 2);
        assert!(page.has_more());
    }

    #[test]
    fn an_empty_listing_still_reports_one_page() {
        let listing = MediaListing {
            page: 0,
            size: 0,
            total: 0,
            list: Vec::new(),
        };
        let page = listing.into_page();

        assert_eq!(page.current_page, 1);
        assert_eq!(page.total_pages, 1);
        assert!(!page.has_more());
    }
}
