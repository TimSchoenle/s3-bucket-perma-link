//! The typed configuration surface, and the dialect of the layered loader it is read through.
//!
//! The layering is [`terrace_config`]'s. Lowest precedence first: struct defaults, TOML at
//! `$S3_PERMA_LINK_CONFIG` (`config.toml` in the working directory when unset),
//! `S3_PERMA_LINK_`-prefixed `__`-nested environment variables, `$S3_PERMA_LINK_SECRETS_DIR`,
//! and `S3_PERMA_LINK_<KEY>_FILE` indirection. See [`loader`] for the details.
//!
//! Call [`load_watched`] rather than [`load`] when the process should be able to pick the
//! configuration up again after a mounted file changes.

mod loader;

use s3::creds::Credentials;
use s3::{Bucket, Region};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::Level;

use crate::error::Error;

pub use loader::{ConfigError, Loaded, Sources, load, load_watched, terrace};

const DEFAULT_SERVER_HOST: &str = "0.0.0.0";
const DEFAULT_SERVER_PORT: u16 = 8080;
const DEFAULT_LOG_LEVEL: &str = "info";

/// Everything the service reads at boot.
#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct Config {
    #[serde(default)]
    server: ServerConfig,
    s3: S3Config,
    bucket: BucketConfig,
    #[serde(default)]
    telemetry: TelemetryConfig,
}

/// Where the object store lives and how to authenticate against it.
#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct S3Config {
    /// A [`SecretString`]: this struct is nested inside a [`Config`] that the reload supervisor
    /// logs, and a credential must not be one `{:?}` away from the log stream.
    access_key: SecretString,
    secret_key: SecretString,
    /// The endpoint, e.g. `s3.eu-central-1.amazonaws.com`.
    host: String,
    region: String,
}

/// The listener the service binds.
#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ServerConfig {
    #[serde(default = "ServerConfig::default_host")]
    host: String,
    #[serde(default = "ServerConfig::default_port")]
    port: u16,
}

/// The routes the service serves, and the object each one resolves to.
#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct BucketConfig {
    /// Request path to object. A table rather than the delimiter-separated string this used to
    /// be: the string existed only because an environment variable cannot carry a map, and the
    /// TOML layer can.
    entries: HashMap<String, BucketEntry>,
}

/// One route's object.
#[derive(Debug, Clone, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct BucketEntry {
    bucket: String,
    /// The object key inside [`Self::bucket`].
    ///
    /// Named `object` rather than `file`, which the loader cannot express: `_FILE` is the
    /// suffix marking file indirection, so `S3_PERMA_LINK_BUCKET__ENTRIES__DOCS__FILE` would be
    /// read as "the value of this key is in the file named by this variable".
    object: String,
}

/// Logging and error reporting.
///
/// Both are installed once, before the reload supervisor is reached, and cannot be reinstalled
/// on a running process — so this is the one block a configuration reload does not apply.
#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct TelemetryConfig {
    /// `trace`, `debug`, `info`, `warn` or `error`. Parsed by [`Self::level`].
    #[serde(default = "TelemetryConfig::default_log_level")]
    log_level: String,
    /// Absent disables Sentry entirely. A [`SecretString`]: a DSN is a write credential for
    /// the project it names.
    #[serde(default)]
    sentry_dsn: Option<SecretString>,
}

impl ServerConfig {
    fn default_host() -> String {
        DEFAULT_SERVER_HOST.to_string()
    }

    fn default_port() -> u16 {
        DEFAULT_SERVER_PORT
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: Self::default_host(),
            port: Self::default_port(),
        }
    }
}

impl TelemetryConfig {
    fn default_log_level() -> String {
        DEFAULT_LOG_LEVEL.to_string()
    }

    /// The configured level.
    ///
    /// # Errors
    /// Returns [`Error::Logger`] if [`Self::log_level`] does not name a level.
    pub fn level(&self) -> crate::Result<Level> {
        Ok(Level::from_str(&self.log_level)?)
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: Self::default_log_level(),
            sentry_dsn: None,
        }
    }
}

impl S3Config {
    pub fn create_buckets(
        &self,
        entries: &HashMap<String, BucketEntry>,
    ) -> crate::Result<HashMap<String, Bucket>> {
        let region = Region::Custom {
            region: self.region.clone(),
            endpoint: self.host.clone(),
        };

        let credentials = Credentials::new(
            Some(self.access_key.expose_secret()),
            Some(self.secret_key.expose_secret()),
            None,
            None,
            None,
        )
        .map_err(|e| Error::custom(format!("Failed to create credentials: {e}")))?;

        let mut buckets = HashMap::new();
        for (key, entry) in entries {
            let bucket = Bucket::new(&entry.bucket, region.clone(), credentials.clone())
                .map_err(|e| {
                    Error::custom(format!("Failed to create bucket {}: {e}", entry.bucket))
                })?
                .with_path_style();
            buckets.insert(key.clone(), *bucket);
        }

        Ok(buckets)
    }
}
