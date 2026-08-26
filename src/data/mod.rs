//! The routing table one generation of the service answers requests from.
//!
//! One value per configuration load, never mutated afterwards: a reload builds a replacement
//! rather than editing this one, so a request in flight keeps the clients it started with.

mod bucket;

pub use bucket::DownloadData;
