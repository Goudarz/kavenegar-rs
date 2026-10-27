//! Paginated results.

use serde::{Deserialize, Serialize};

use crate::de::lenient_i64;

/// A page of results, plus the counters needed to fetch the next one.
///
/// Dereferences to a slice, so the common case reads like a `Vec`:
///
/// ```no_run
/// # async fn run(client: kavenegar::Kavenegar) -> Result<(), kavenegar::Error> {
/// let page = client.verify().templates().await?;
/// for template in &page {
///     println!("{}", template.name);
/// }
/// if let Some(next) = page.next_page() {
///     let following = client.verify().templates().page(next).await?;
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Page<T> {
    /// The records on this page.
    pub items: Vec<T>,
    /// Total records across every page.
    pub total_count: i64,
    /// This page's number, counting from 1.
    pub current_page: i64,
    /// How many pages exist in total.
    pub total_pages: i64,
    /// How many records fit on a page.
    pub page_size: i64,
}

impl<T> Page<T> {
    /// Whether another page follows this one.
    pub fn has_more(&self) -> bool {
        self.current_page < self.total_pages
    }

    /// The number to pass to the next request's `page`, if there is a next
    /// page.
    pub fn next_page(&self) -> Option<u32> {
        if !self.has_more() {
            return None;
        }
        self.current_page
            .checked_add(1)
            .and_then(|next| u32::try_from(next).ok())
    }

    /// Drops the pagination counters, keeping only the records.
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
}

impl<T> std::ops::Deref for Page<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<T> IntoIterator for Page<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Page<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// The `metadata` block that accompanies a paginated response.
///
/// Kavenegar sends these counters as JSON strings on some endpoints and as
/// numbers on others, and casts the field names differently again, so every
/// field accepts both spellings and both types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct PageMeta {
    /// Total records across every page.
    #[serde(
        rename = "totalcount",
        alias = "TotalCount",
        default,
        deserialize_with = "lenient_i64"
    )]
    pub total_count: i64,
    /// This page's number, counting from 1.
    #[serde(
        rename = "currentpage",
        alias = "CurrentPage",
        default,
        deserialize_with = "lenient_i64"
    )]
    pub current_page: i64,
    /// How many pages exist in total.
    #[serde(
        rename = "totalpages",
        alias = "TotalPages",
        default,
        deserialize_with = "lenient_i64"
    )]
    pub total_pages: i64,
    /// How many records fit on a page.
    #[serde(
        rename = "pagesize",
        alias = "PageSize",
        default,
        deserialize_with = "lenient_i64"
    )]
    pub page_size: i64,
}

impl PageMeta {
    /// Attaches these counters to a set of records.
    #[cfg(any(feature = "sms", feature = "verify", feature = "line"))]
    pub(crate) fn into_page<T>(self, items: Vec<T>) -> Page<T> {
        Page {
            items,
            total_count: self.total_count,
            current_page: self.current_page,
            total_pages: self.total_pages,
            page_size: self.page_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_accepts_strings_and_numbers() {
        let as_strings: PageMeta = serde_json::from_str(
            r#"{"totalcount":"120","currentpage":"2","totalpages":"3","pagesize":"50"}"#,
        )
        .unwrap();
        let as_numbers: PageMeta = serde_json::from_str(
            r#"{"totalcount":120,"currentpage":2,"totalpages":3,"pagesize":50}"#,
        )
        .unwrap();

        assert_eq!(as_strings, as_numbers);
        assert_eq!(as_strings.total_count, 120);
    }

    #[test]
    fn metadata_accepts_the_pascal_case_spelling() {
        let meta: PageMeta =
            serde_json::from_str(r#"{"TotalCount":"7","CurrentPage":"1","TotalPages":"1"}"#)
                .unwrap();
        assert_eq!(meta.total_count, 7);
        assert_eq!(meta.page_size, 0);
    }

    #[cfg(any(feature = "sms", feature = "verify", feature = "line"))]
    #[test]
    fn pagination_helpers_track_position() {
        let meta = PageMeta {
            total_count: 120,
            current_page: 2,
            total_pages: 3,
            page_size: 50,
        };
        let page = meta.into_page(vec!["a", "b"]);

        assert_eq!(page.len(), 2);
        assert_eq!(page[0], "a");
        assert!(page.has_more());
        assert_eq!(page.next_page(), Some(3));

        let last = PageMeta {
            current_page: 3,
            total_pages: 3,
            ..meta
        }
        .into_page(vec!["c"]);
        assert!(!last.has_more());
        assert_eq!(last.next_page(), None);
    }
}
