//! The `s3-bucket-perma-link` dialect of [`terrace_config`].
//!
//! The layering itself — the TOML fragments, the `S3_PERMA_LINK_*` environment layer, the
//! secrets-directory provider, the `_FILE` indirection and the shadow-key rejection — belongs
//! to `terrace-config`. What stays here is the one thing that is ours: which environment names
//! this deployment spells.

use serde::de::DeserializeOwned;
use terrace_config::Terrace;

pub use terrace_config::{Error as ConfigError, Loaded, Sources};

/// The prefix every configuration variable carries.
const PREFIX: &str = "S3_PERMA_LINK_";

/// The loader the service boots through.
///
/// Layers, lowest precedence first: struct defaults, TOML at `$S3_PERMA_LINK_CONFIG` (a file,
/// or every `*.toml` in it if it names a directory, falling back to `config.toml` in the
/// working directory), `S3_PERMA_LINK_`-prefixed `__`-nested environment variables,
/// `$S3_PERMA_LINK_SECRETS_DIR`, and `S3_PERMA_LINK_<KEY>_FILE` indirection. The last three are
/// mutually exclusive per key: a key supplied by two of them is refused at boot rather than
/// resolved by precedence, because a stale environment variable shadowing a rotated mounted
/// secret keeps the service running on the old credential.
///
/// Nothing is reserved beyond `S3_PERMA_LINK_CONFIG` and `S3_PERMA_LINK_SECRETS_DIR`, which
/// `terrace-config` reserves itself: every value this service reads, the Sentry DSN and the log
/// level included, is read out of the layers rather than out of the environment directly, so
/// there is no key a mounted file could supply to nobody.
#[must_use]
pub fn terrace() -> Terrace {
    Terrace::new(PREFIX)
}

/// Load a typed config.
///
/// # Errors
/// Returns [`ConfigError`] if a required value is missing, a value fails to parse, a
/// file-backed source cannot be read, or one key is supplied by more than one of the last
/// three layers.
pub fn load<T: DeserializeOwned>() -> Result<T, ConfigError> {
    terrace().load()
}

/// Load a typed config and everything a reload needs to load it again.
///
/// # Errors
/// As [`load`].
pub fn load_watched<T: DeserializeOwned>() -> Result<Loaded<T>, ConfigError> {
    terrace().load_watched()
}

#[cfg(test)]
mod tests {
    use super::load;
    use crate::config::Config;
    use secrecy::ExposeSecret;

    /// Set the two S3 credentials every load needs, so a test can say only what it is about.
    fn set_credentials(jail: &mut figment::Jail) {
        jail.set_env("S3_PERMA_LINK_S3__ACCESS_KEY", "access");
        jail.set_env("S3_PERMA_LINK_S3__SECRET_KEY", "secret");
        jail.set_env("S3_PERMA_LINK_S3__HOST", "s3.example.com");
        jail.set_env("S3_PERMA_LINK_S3__REGION", "eu-central-1");
    }

    /// The dialect, end to end: the prefix, the `__` nesting, and the defaults that fill in
    /// around what the environment supplied. `terrace-config` owns the layering and tests it;
    /// what this pins is that the service wires it to the names an operator actually sets.
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail::expect_with fixes the closure's error type"
    )]
    fn env_overrides_and_defaults_apply() {
        figment::Jail::expect_with(|jail| {
            jail.clear_env();
            set_credentials(jail);
            jail.set_env("S3_PERMA_LINK_BUCKET__ENTRIES__docs__BUCKET", "media");
            jail.set_env(
                "S3_PERMA_LINK_BUCKET__ENTRIES__docs__OBJECT",
                "handbook.pdf",
            );
            jail.set_env("S3_PERMA_LINK_SERVER__PORT", "9090");

            let config: Config = load().map_err(|e| e.to_string()).unwrap();

            // `SecretString` has no `PartialEq`; comparing requires `expose_secret()`.
            assert_eq!(config.s3().access_key().expose_secret(), "access");
            assert_eq!(*config.server().port(), 9090);
            // Untouched neighbour in the same block still gets its default.
            assert_eq!(config.server().host(), "0.0.0.0");
            // A block untouched by the environment still materialises with its own defaults.
            assert_eq!(config.telemetry().log_level(), "info");
            assert!(config.telemetry().sentry_dsn().is_none());

            let entry = config.bucket().entries().get("docs").expect("docs entry");
            assert_eq!(entry.bucket(), "media");
            assert_eq!(entry.object(), "handbook.pdf");
            Ok(())
        });
    }

    /// The bucket map is a real table in the TOML layer, which is the whole point of the file
    /// layers: the entries used to be squeezed into one `key:bucket,file; ...` string because
    /// an environment variable cannot carry a map.
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail::expect_with fixes the closure's error type"
    )]
    fn the_toml_layer_carries_the_bucket_table() {
        figment::Jail::expect_with(|jail| {
            jail.clear_env();
            set_credentials(jail);
            // No `S3_PERMA_LINK_CONFIG`: this also pins the `config.toml` fallback, which is
            // what a container with a `ConfigMap` mounted over its working directory uses.
            jail.create_file(
                "config.toml",
                r#"
[bucket.entries.docs]
bucket = "media"
object = "handbook.pdf"

[bucket.entries."release-notes"]
bucket = "media"
object = "CHANGELOG.md"
"#,
            )?;

            let config: Config = load().map_err(|e| e.to_string()).unwrap();

            assert_eq!(config.bucket().entries().len(), 2);
            // A key an environment variable could not have spelled: the environment layer
            // lowercases, and `-` is not expressible in a shell variable name.
            assert_eq!(
                config
                    .bucket()
                    .entries()
                    .get("release-notes")
                    .expect("release-notes entry")
                    .object(),
                "CHANGELOG.md"
            );
            Ok(())
        });
    }

    /// A mounted secret outranks the TOML layer, so a `ConfigMap` carrying a placeholder key
    /// cannot win over the `Secret` that carries the real one — through the variable names
    /// *this* crate configures, which is the half a dependency cannot pin.
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail::expect_with fixes the closure's error type"
    )]
    fn a_secrets_directory_outranks_the_toml_layer() {
        figment::Jail::expect_with(|jail| {
            jail.clear_env();
            jail.create_file(
                "config.toml",
                r#"
[s3]
access_key = "placeholder"
secret_key = "placeholder"
host = "s3.example.com"
region = "eu-central-1"

[bucket.entries.docs]
bucket = "media"
object = "handbook.pdf"
"#,
            )?;
            jail.create_dir("secrets")?;
            jail.create_file("secrets/s3__secret_key", "rotated\n")?;
            jail.set_env(
                "S3_PERMA_LINK_SECRETS_DIR",
                jail.directory().join("secrets").display(),
            );

            let config: Config = load().map_err(|e| e.to_string()).unwrap();

            assert_eq!(config.s3().secret_key().expose_secret(), "rotated");
            // The key the mount said nothing about still comes from the TOML layer.
            assert_eq!(config.s3().access_key().expose_secret(), "placeholder");
            Ok(())
        });
    }

    /// A single value can also be pointed at a file by name, which is the `_FILE` convention a
    /// Docker Compose stack spells rather than mounting a whole secrets directory.
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail::expect_with fixes the closure's error type"
    )]
    fn a_file_indirection_variable_supplies_one_key() {
        figment::Jail::expect_with(|jail| {
            jail.clear_env();
            // Everything but the secret, which the indirection variable supplies instead.
            jail.set_env("S3_PERMA_LINK_S3__ACCESS_KEY", "access");
            jail.set_env("S3_PERMA_LINK_S3__HOST", "s3.example.com");
            jail.set_env("S3_PERMA_LINK_S3__REGION", "eu-central-1");
            jail.set_env("S3_PERMA_LINK_BUCKET__ENTRIES__docs__BUCKET", "media");
            jail.set_env(
                "S3_PERMA_LINK_BUCKET__ENTRIES__docs__OBJECT",
                "handbook.pdf",
            );

            jail.create_file("secret_key", "from-a-file\n")?;
            // The value names the *path*; the key it fills is the one before `_FILE`.
            jail.set_env(
                "S3_PERMA_LINK_S3__SECRET_KEY_FILE",
                jail.directory().join("secret_key").display(),
            );

            let config: Config = load().map_err(|e| e.to_string()).unwrap();

            // Trailing newline trimmed: an editor or a `kubectl create secret --from-file`
            // adds one, and it is not part of the credential.
            assert_eq!(config.s3().secret_key().expose_secret(), "from-a-file");
            Ok(())
        });
    }

    /// One key supplied by both the environment and a mounted file fails the boot instead of
    /// being resolved by precedence: the environment variable is the one that cannot be
    /// rotated, so silently preferring either is how a service keeps serving on a credential
    /// its operator believes they replaced.
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail::expect_with fixes the closure's error type"
    )]
    fn a_key_supplied_twice_is_refused() {
        figment::Jail::expect_with(|jail| {
            jail.clear_env();
            set_credentials(jail);
            jail.create_dir("secrets")?;
            jail.create_file("secrets/s3__secret_key", "rotated")?;
            jail.set_env(
                "S3_PERMA_LINK_SECRETS_DIR",
                jail.directory().join("secrets").display(),
            );

            let error = load::<Config>().expect_err("a shadowed key must fail the boot");
            assert!(
                error.to_string().to_lowercase().contains("secret_key"),
                "the error must name the key: {error}"
            );
            Ok(())
        });
    }
}
