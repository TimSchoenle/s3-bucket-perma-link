//! Dump the configuration surface of `Config`, for documentation, for editor tooling and for the
//! container build.
//!
//! The loader hands a merged figment to `serde` and takes back a `Config`; it never learns the
//! shape of one. `terrace-config`'s `schema` feature inverts that, so every artefact describing
//! the configuration is generated from the type in `src/config.rs` rather than maintained beside
//! it and left to drift:
//!
//! ```text
//! cargo run --example config-schema -- --format markdown    # the README tables
//! cargo run --example config-schema -- --format json        # the machine-readable schema
//! cargo run --example config-schema -- --format json-schema # for an editor to validate against
//! cargo run --example config-schema -- --format contract    > contract.json
//! cargo run --example config-schema -- --format labels      > contract.labels
//! cargo run --example config-schema -- --format dockerfile  # paste into the Dockerfile
//! ```
//!
//! `--format markdown` is the one the README is built from — `examples/readme-variables.rs`
//! embeds the same tables into the template. Run it here to read them on their own.
//!
//! # The build outputs
//!
//! `contract`, `labels` and `dockerfile` describe the *image* rather than a page about it, and
//! the three are one set: the document is copied into the image and attached to its digest, the
//! labels are what let a chart find it without pulling a layer, and the `LABEL` block is those
//! labels in the form a Dockerfile can carry. Generate the first two from one run of this
//! program in one build — that is the only thing that makes it impossible for them to disagree —
//! and check the built image against the second, because a hand-pasted `LABEL` block with
//! nothing checking it is exactly the failure the labels exist to rule out.
//! `.github/scripts/check-contract-labels.sh` is that check.
//!
//! Nothing below reads the environment, so the output is the same on a developer's machine as on
//! a runner where none of the variables it describes exist. `--version`, `--revision` and
//! `--created` are the three things that legitimately differ between builds of one source tree,
//! and they are flags for that reason: omitted, `--format contract` is byte-reproducible, which
//! is what lets `docs/config.contract.json` be committed and diffed in review.

use std::process::ExitCode;

use s3_bucket_perma_link::config;
use terrace_config::schema::{Contract, DEFAULT_PATH, JsonSchema};

/// The `$id` the generated JSON Schema carries.
const SCHEMA_ID: &str = "https://github.com/TimSchoenle/s3-bucket-perma-link/config.schema.json";

fn main() -> ExitCode {
    let options = match Options::from_args() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    match render(&options) {
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

fn render(options: &Options) -> Result<String, config::ConfigError> {
    match options.format {
        Format::Markdown => Ok(config::schema()?.to_markdown()),
        Format::Json => config::schema()?.to_json(),
        Format::JsonSchema => config::schema()?.to_json_schema_with(
            &JsonSchema::new()
                .title("s3-bucket-perma-link configuration")
                .id(SCHEMA_ID),
        ),
        // A trailing newline on each of the three, because every one of them is redirected into
        // a file that another tool reads line by line or byte for byte.
        Format::Contract => Ok(format!("{}\n", contract(options)?.to_json()?)),
        Format::Labels => Ok(contract(options)?
            .labels(DEFAULT_PATH)
            .into_iter()
            .map(|(name, value)| format!("{name}={value}\n"))
            .collect()),
        Format::Dockerfile => Ok(contract(options)?.to_dockerfile_labels(DEFAULT_PATH)),
    }
}

/// The contract this build publishes.
///
/// The configuration surface and the external variables come from `src/config.rs`, beside the
/// code that reads them. Only what makes this a *build* rather than a source tree is assembled
/// here.
fn contract(options: &Options) -> Result<Contract, config::ConfigError> {
    let mut app = config::app();
    if let Some(version) = &options.version {
        app = app.version(version);
    }
    if let Some(revision) = &options.revision {
        app = app.revision(revision);
    }
    if let Some(created) = &options.created {
        app = app.created(created);
    }
    config::contract(app)
}

/// What to emit, and what this build is.
struct Options {
    format: Format,
    /// The release this build is of, spelled as the image tag spells it — `v1.0.1`, not `1.0.1`.
    version: Option<String>,
    /// The commit this build is of.
    revision: Option<String>,
    /// When this build happened, RFC 3339.
    created: Option<String>,
}

/// Which rendering to emit.
#[derive(Clone, Copy)]
enum Format {
    /// GitHub-flavoured tables, for a pipeline whose next step is a README.
    Markdown,
    /// The versioned schema, for a pipeline that renders its own tables.
    Json,
    /// A JSON Schema, for an editor to validate a `config.toml` against.
    JsonSchema,
    /// The document a build embeds in its image and attaches to its digest.
    Contract,
    /// The image labels that make that document discoverable, one `NAME=value` per line.
    Labels,
    /// The same labels as the `LABEL` instruction to paste into a Dockerfile.
    Dockerfile,
}

impl Options {
    /// JSON unless asked otherwise: it is the rendering that loses nothing.
    fn from_args() -> Result<Self, String> {
        let mut options = Self {
            format: Format::Json,
            version: None,
            revision: None,
            created: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--format" => {
                    options.format = match args.next().as_deref() {
                        Some("markdown" | "md") => Format::Markdown,
                        Some("json") => Format::Json,
                        Some("json-schema" | "jsonschema") => Format::JsonSchema,
                        Some("contract") => Format::Contract,
                        Some("labels") => Format::Labels,
                        Some("dockerfile") => Format::Dockerfile,
                        Some(other) => return Err(format!("unknown format `{other}`; {USAGE}")),
                        None => return Err(format!("--format takes a value; {USAGE}")),
                    };
                }
                "--version" => options.version = Some(value(&mut args, "--version", "a release")?),
                "--revision" => {
                    options.revision = Some(value(&mut args, "--revision", "a commit")?);
                }
                "--created" => {
                    options.created = Some(value(&mut args, "--created", "a timestamp")?);
                }
                other => return Err(format!("unknown argument `{other}`; {USAGE}")),
            }
        }
        Ok(options)
    }
}

/// The argument after `flag`, or a message naming what was expected.
///
/// An empty value is refused rather than recorded: a build passing `--version "$VERSION"` with
/// nothing in `VERSION` means the argument failed to interpolate, and a contract claiming the
/// empty release is worse than a build that stops.
fn value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
    expected: &str,
) -> Result<String, String> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} takes {expected}; {USAGE}"))
}

const USAGE: &str = "usage: config-schema \
                     [--format markdown|json|json-schema|contract|labels|dockerfile] \
                     [--version <release>] [--revision <commit>] [--created <rfc3339>]";
