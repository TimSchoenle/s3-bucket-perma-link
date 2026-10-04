use crate::config::BucketEntry;
use s3::Bucket;
use std::collections::HashMap;

/// One bucket client and one entry per configured request path.
///
/// Both maps are keyed by request path and built from the same `bucket.entries`, so a key in one
/// is a key in the other.
#[derive(Getters)]
#[getset(get = "pub")]
pub struct DownloadData {
    /// The client each request path is served through.
    buckets: HashMap<String, Bucket>,
    /// The bucket and object key each request path resolves to.
    bucket_config: HashMap<String, BucketEntry>,
}

impl DownloadData {
    /// Pairs each request path's bucket client with the entry it resolves to.
    ///
    /// The caller keeps the two maps keyed identically; see the type documentation.
    #[must_use]
    pub fn new(
        buckets: HashMap<String, Bucket>,
        bucket_config: HashMap<String, BucketEntry>,
    ) -> Self {
        Self {
            buckets,
            bucket_config,
        }
    }

    /// Returns the entry bound to `key`, or `None` when no permanent link is configured under it.
    ///
    /// # Examples
    /// ```
    /// # use s3_bucket_perma_link::{config::BucketEntry, data::DownloadData};
    /// # use std::collections::HashMap;
    /// let entry: BucketEntry =
    ///     serde_json::from_str(r#"{"bucket": "media", "object": "handbook.pdf"}"#)?;
    /// let entries = HashMap::from([("docs/handbook".to_owned(), entry)]);
    /// let data = DownloadData::new(HashMap::new(), entries);
    ///
    /// assert!(data.get_entry("docs/handbook").is_some());
    /// assert!(data.get_entry("/docs/handbook").is_none());
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    #[must_use]
    pub fn get_entry(&self, key: &str) -> Option<&BucketEntry> {
        self.bucket_config.get(key)
    }
}
