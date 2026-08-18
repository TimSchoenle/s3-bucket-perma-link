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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use terrace_config::schema::{Describe, Schema};
use tracing::Level;

use crate::error::Error;

pub use loader::{ConfigError, Loaded, Sources, explain, load, load_watched, terrace};

const DEFAULT_SERVER_HOST: &str = "0.0.0.0";
const DEFAULT_SERVER_PORT: u16 = 8080;
const DEFAULT_LOG_LEVEL: &str = "info";

/// Everything the service reads at boot.
///
/// [`Describe`] is what the configuration reference in `README.md` is generated from: the key
/// paths, the environment spellings, the types and the `///` comments below are read off this
/// tree by `examples/config-schema.rs` rather than restated in prose that can drift from it.
#[derive(Debug, Deserialize, Serialize, Getters, Describe)]
#[getset(get = "pub")]
pub struct Config {
    #[config(nested)]
    #[serde(default)]
    server: ServerConfig,
    #[config(nested)]
    s3: S3Config,
    #[config(nested)]
    bucket: BucketConfig,
    #[config(nested)]
    #[serde(default)]
    telemetry: TelemetryConfig,
}

/// What the schema generator reads the `Default` column out of.
///
/// It is not a configuration a service could run on: [`S3Config`]'s fields are required, and a
/// required key reports no default at all, so nothing here reaches the generated table. Only the
/// blocks that really do have defaults — [`ServerConfig`] and [`TelemetryConfig`] — contribute.
impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            s3: S3Config::default(),
            bucket: BucketConfig::default(),
            telemetry: TelemetryConfig::default(),
        }
    }
}

/// Where the object store lives and how to authenticate against it.
#[derive(Debug, Deserialize, Serialize, Default, Getters, Describe)]
#[getset(get = "pub")]
pub struct S3Config {
    /// S3 access key. Mount it rather than setting it in a file that is committed.
    ///
    /// A [`SecretString`]: this struct is nested inside a [`Config`] that the reload supervisor
    /// logs, and a credential must not be one `{:?}` away from the log stream. `SecretString`
    /// refuses to implement [`Serialize`] for the same reason, which is why the field is skipped
    /// on the way out — `#[config(secret)]` renders `<redacted>` in its place anyway.
    #[config(secret)]
    #[serde(skip_serializing)]
    access_key: SecretString,
    /// S3 secret key. Mount it rather than setting it in a file that is committed.
    #[config(secret)]
    #[serde(skip_serializing)]
    secret_key: SecretString,
    /// The endpoint, e.g. `s3.eu-central-1.amazonaws.com`.
    host: String,
    /// The region the endpoint serves, e.g. `eu-central-1`.
    region: String,
}

/// The listener the service binds.
#[derive(Debug, Deserialize, Serialize, Getters, Describe)]
#[getset(get = "pub")]
pub struct ServerConfig {
    /// Address to listen on. `0.0.0.0` in a container, which is the deployment this ships as.
    #[serde(default = "ServerConfig::default_host")]
    host: String,
    /// Port to listen on.
    #[serde(default = "ServerConfig::default_port")]
    port: u16,
}

/// The routes the service serves, and the object each one resolves to.
#[derive(Debug, Deserialize, Serialize, Default, Getters, Describe)]
#[getset(get = "pub")]
pub struct BucketConfig {
    /// One `[bucket.entries.<request path>]` block per permanent link, each carrying a `bucket`
    /// and an `object`.
    ///
    /// A leaf rather than `#[config(nested)]`, because the key paths under it are the operator's
    /// route names and no type knows them ahead of time — see [`BucketEntry`] for the two fields
    /// each block takes. A table rather than the delimiter-separated string this used to be: the
    /// string existed only because an environment variable cannot carry a map, and the TOML layer
    /// can.
    entries: HashMap<String, BucketEntry>,
}

/// One route's object.
#[derive(Debug, Clone, Deserialize, Serialize, Getters)]
#[getset(get = "pub")]
pub struct BucketEntry {
    /// The bucket [`Self::object`] lives in.
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
#[derive(Debug, Deserialize, Serialize, Getters, Describe)]
#[getset(get = "pub")]
pub struct TelemetryConfig {
    /// How much the service says: `trace`, `debug`, `info`, `warn` or `error`.
    ///
    /// Parsed by [`Self::level`].
    #[serde(default = "TelemetryConfig::default_log_level")]
    log_level: String,
    /// Sentry DSN. Absent disables Sentry entirely.
    ///
    /// A [`SecretString`]: a DSN is a write credential for the project it names — and, like the
    /// S3 credentials, one serde must not be able to write back out.
    #[config(secret)]
    #[serde(default, skip_serializing)]
    sentry_dsn: Option<SecretString>,
}

/// The configuration surface, described rather than deserialised.
///
/// The reference tables in `README.md` are rendered from this: the key paths, the environment
/// spellings, the types, the defaults and the `///` comments all come off [`Config`], so the
/// documentation cannot say something the type does not. `examples/config-schema.rs` and
/// `examples/readme-variables.rs` are the two callers; both go through here so that neither can
/// describe a different loader than the service boots with.
///
/// Reads nothing from the environment — the `Default` column is filled from [`Config::default`],
/// so a documentation job produces the same answer on a runner where none of these variables
/// exist. Required keys report no default at all, which is why the empty S3 credentials
/// [`Config::default`] carries never reach the table.
///
/// # Errors
/// Returns [`ConfigError`] if [`Config`] does not serialise.
pub fn schema() -> Result<Schema, ConfigError> {
    terrace()
        .schema::<Config>()
        .with_defaults_from(&Config::default())
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
