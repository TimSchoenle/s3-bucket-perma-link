use actix_web::http::Method;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Tracing error")]
    Logger(#[from] tracing::metadata::ParseLevelError),
    // Both telemetry variants carry their message for the same reason the config ones do: they
    // are raised at boot, before anything is serving, and the text names the key an operator
    // has to change. Neither ever carries the DSN itself — see `telemetry::sentry::init`.
    #[error("Tracing error: {0}")]
    Tracing(String),
    #[cfg(feature = "sentry")]
    #[error("Sentry error: {0}")]
    Sentry(String),
    #[error("IO error")]
    IoError(#[from] std::io::Error),
    #[error("Invalid route")]
    InvalidRoute(String),
    // The message is carried through, unlike the variants around it: a configuration failure
    // names the key, the file or the mount an operator has to fix, and this is also what the
    // reload supervisor prints when a re-read fails on a service that is still serving.
    #[error("Config error: {0}")]
    Config(#[from] terrace_config::Error),
    #[error("Config watch error: {0}")]
    ConfigWatch(#[from] terrace_config::reload::WatchError),
    #[error("Bukkit error")]
    S3(#[from] s3::error::S3Error),
    #[error("{0}")]
    Custom(String),
}

impl Error {
    pub fn custom<S: ToString>(msg: S) -> Self {
        Self::Custom(msg.to_string())
    }

    pub fn invalid_route(route: &Method) -> Self {
        Self::InvalidRoute(route.to_string())
    }
}
