//! Dump the configuration surface of `Config`, for documentation and for editor tooling.
//!
//! The loader hands a merged figment to `serde` and takes back a `Config`; it never learns the
//! shape of one. `terrace-config`'s `schema` feature inverts that, so every artefact describing
//! the configuration is generated from the type in `src/config.rs` rather than maintained beside
//! it and left to drift:
//!
//! ```text
//! cargo run --example config-schema -- --format markdown    # the README tables
//! cargo run --example config-schema -- --format json        # the machine-readable contract
//! cargo run --example config-schema -- --format json-schema # for an editor to validate against
//! ```
//!
//! `--format markdown` is the one the README is built from — `examples/readme-variables.rs`
//! embeds the same tables into the template. Run it here to read them on their own.
//!
//! Nothing below reads the environment, so the output is the same on a developer's machine as on
//! a runner where none of the variables it describes exist.

use std::process::ExitCode;

use s3_bucket_perma_link::config;
use terrace_config::schema::JsonSchema;

/// The `$id` the generated JSON Schema carries.
const SCHEMA_ID: &str = "https://github.com/TimSchoenle/s3-bucket-perma-link/config.schema.json";

fn main() -> ExitCode {
    let format = match Format::from_args() {
        Ok(format) => format,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    match render(format) {
        Ok(rendered) => {
            print!("{rendered}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn render(format: Format) -> Result<String, config::ConfigError> {
    let schema = config::schema()?;

    match format {
        Format::Markdown => Ok(schema.to_markdown()),
        Format::Json => schema.to_json(),
        Format::JsonSchema => schema.to_json_schema_with(
            &JsonSchema::new()
                .title("s3-bucket-perma-link configuration")
                .id(SCHEMA_ID),
        ),
    }
}

/// Which rendering to emit.
enum Format {
    /// GitHub-flavoured tables, for a pipeline whose next step is a README.
    Markdown,
    /// The versioned contract, for a pipeline that renders its own tables.
    Json,
    /// A JSON Schema, for an editor to validate a `config.toml` against.
    JsonSchema,
}

impl Format {
    /// JSON unless asked otherwise: it is the rendering that loses nothing.
    fn from_args() -> Result<Self, String> {
        let mut format = Self::Json;
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--format" => {
                    format = match args.next().as_deref() {
                        Some("markdown" | "md") => Self::Markdown,
                        Some("json") => Self::Json,
                        Some("json-schema" | "jsonschema") => Self::JsonSchema,
                        Some(other) => return Err(format!("unknown format `{other}`; {USAGE}")),
                        None => return Err(format!("--format takes a value; {USAGE}")),
                    };
                }
                other => return Err(format!("unknown argument `{other}`; {USAGE}")),
            }
        }
        Ok(format)
    }
}

const USAGE: &str = "usage: config-schema [--format markdown|json|json-schema]";
