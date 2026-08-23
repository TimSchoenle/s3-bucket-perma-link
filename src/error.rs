//! The crate's error type, and the text an operator sees when the process gives up.

use actix_web::http::Method;
use thiserror::Error;

/// Every failure this crate reports, from a configuration that will not load to a bucket the
/// credentials could not address.
#[derive(Error, Debug)]
pub enum Error {
    /// `telemetry.log_level` does not name a tracing level.
    #[error("Tracing error")]
    Logger(#[from] tracing::metadata::ParseLevelError),
    /// The global `tracing` subscriber could not be installed, because one already is.
    ///
    /// Only [`crate::telemetry::init`] installs one, and only once per process, so this is a
    /// second call rather than a configuration mistake.
    // Both telemetry variants carry their message for the same reason the config ones do: they
    // are raised at boot, before anything is serving, and the text names the key an operator
    // has to change. Neither ever carries the DSN itself — see `telemetry::sentry::init`.
    #[error("Tracing error: {0}")]
    Tracing(String),
    /// `telemetry.sentry` is switched on but unusable: no DSN, a DSN that does not parse, or a
    /// sample rate outside `0.0..=1.0`.
    #[cfg(feature = "sentry")]
    #[error("Sentry error: {0}")]
    Sentry(String),
    /// The listener could not bind `server.host` and `server.port`, or failed while running.
    #[error("IO error")]
    IoError(#[from] std::io::Error),
    /// A method the matched route does not serve.
    ///
    /// Nothing constructs it: actix answers a method a resource has no route for with 405,
    /// before any handler in this crate runs.
    #[error("Invalid route")]
    InvalidRoute(String),
    /// The configuration was refused: a required key is missing, a value did not parse, a
    /// file-backed layer could not be read, or one key was supplied by two of the last three
    /// layers.
    // The message is carried through, unlike the variants around it: a configuration failure
    // names the key, the file or the mount an operator has to fix, and this is also what the
    // reload supervisor prints when a re-read fails on a service that is still serving.
    #[error("Config error: {0}")]
    Config(#[from] terrace_config::Error),
    /// The supervisor could not watch the files the configuration was loaded from, so a rotated
    /// secret would never be noticed.
    #[error("Config watch error: {0}")]
    ConfigWatch(#[from] terrace_config::reload::WatchError),
    /// The object store refused a request.
    ///
    /// Nothing propagates one today. A download that fails against the bucket is turned into a
    /// 500 in the handler, where the request that caused it is still in scope.
    #[error("Bukkit error")]
    S3(#[from] s3::error::S3Error),
    /// A failure with no type of its own, carrying the message an operator reads.
    ///
    /// [`S3Config::create_buckets`](crate::config::S3Config::create_buckets) is the only source:
    /// credentials the S3 client rejected, or a bucket name it could not address.
    #[error("{0}")]
    Custom(String),
}

impl Error {
    /// An [`Error::Custom`] whose message reaches the operator verbatim, so `msg` has to name
    /// the key, the bucket or the file that failed.
    #[must_use]
    pub fn custom(msg: impl Into<String>) -> Self {
        Self::Custom(msg.into())
    }

    /// An [`Error::InvalidRoute`] naming the HTTP method that was refused.
    #[must_use]
    pub fn invalid_route(route: &Method) -> Self {
        Self::InvalidRoute(route.to_string())
    }
}
