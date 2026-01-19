use crate::config::BucketEntry;
use derive_new::new;
use s3::Bucket;
use std::collections::HashMap;

#[derive(Getters, new)]
#[getset(get = "pub")]
pub struct DownloadData {
    buckets: HashMap<String, Bucket>,
    bucket_config: HashMap<String, BucketEntry>,
}

impl DownloadData {
    pub fn get_entry(&self, key: &str) -> Option<&BucketEntry> {
        self.bucket_config.get(key)
    }
}
