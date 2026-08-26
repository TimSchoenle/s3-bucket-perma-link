//! A read-only front for objects in an S3-compatible store, at request paths an operator picks.
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
//! `examples/readme-variables.rs` link it for [`config::schema`], [`config::app`],
//! [`config::external`] and [`config::terrace`], which is how the README's configuration tables,
//! `docs/config.contract.json` and the `LABEL` block in the `Dockerfile` are rendered from the
//! types the service deserialises into.

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

/// A result whose error is always [`Error`], never an erased `anyhow::Error`.
pub type Result<T> = anyhow::Result<T, Error>;
