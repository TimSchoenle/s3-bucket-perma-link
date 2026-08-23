//! A read-only front for an S3 bucket, under request paths an operator chooses.
//!
//! [`data::DownloadData`] is `bucket.entries` resolved into one client per request path, and a
//! path with no entry is refused before any S3 call.
//!
//! [`config`] owns the typed configuration and the loader dialect, [`server`] the listener,
//! [`shutdown`] the signal that stops it, [`data`] the routing table one generation serves from,
//! and [`mod@error`] the single error type the rest of them return.
//!
//! # Everything here is rebuilt when the configuration changes
//!
//! The binary runs the loader under `terrace_config::reload`, which re-reads the configuration
//! when a watched file changes and then builds a fresh [`data::DownloadData`] and a fresh
//! [`server::Server`] around the result. That is what lets a rotated S3 credential be picked up
//! without a restart, and it is why nothing in this crate keeps state across a generation. The
//! two things that do are the tracing subscriber and the Sentry client, installed by `main`
//! before the supervisor exists, which is why `telemetry.*` is the one configuration block a
//! reload cannot apply.
//!
//! # It is a library so that the documentation can be generated from it
//!
//! At run time the binary is the only consumer. `examples/config-schema.rs` and
//! `examples/readme-variables.rs` link it for [`config::schema`] alone: the README's
//! configuration tables, `docs/config.contract.json` and the `LABEL` block in the `Dockerfile`
//! are all rendered from the types the service deserialises into, so none of them can describe a
//! loader other than the one that boots.

#[macro_use]
extern crate getset;
#[macro_use]
extern crate tracing;

use crate::error::Error;
pub mod config;
pub mod data;
pub mod error;
mod routes;
pub mod server;
pub mod shutdown;
pub mod telemetry;

/// A result carrying [`Error`].
///
/// `anyhow::Result<T, E>` is `core::result::Result<T, E>` with the second parameter filled in, so
/// a caller can match on the variants.
pub type Result<T> = anyhow::Result<T, Error>;
