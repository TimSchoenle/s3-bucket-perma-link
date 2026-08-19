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
//! cargo run --example config-schema -- --format contract    > docs/config.contract.json
//! cargo run --example config-schema -- --format labels      > contract.labels
//! cargo run --example config-schema -- --format dockerfile  # paste into the Dockerfile
//! ```
//!
//! `--format markdown` is the one the README is built from — `examples/readme-variables.rs`
//! embeds the same tables into the template. Run it here to read them on their own.
//!
//! # What is left here
//!
//! The `--format` vocabulary, the argument parsing, the dispatch across the six renderings, the
//! printing and the exit code are [`Cli`](terrace_config::schema::cli::Cli). They were the same
//! two hundred lines in every repository that had a generator, which is how three of them ended
//! up disagreeing about how to cut a `LABEL` block back out of a Dockerfile.
//!
//! What is genuinely this service's own is below, and all of it comes from `src/config.rs`, beside
//! the code that reads the values: the schema, the app identity, the JSON Schema's `$id`, and the
//! external surface no derive can find. A documentation job and a container build therefore cannot
//! describe different loaders.
//!
//! # The build outputs
//!
//! `contract`, `labels` and `dockerfile` describe the *image* rather than a page about it, and
//! the three are one set: the document is copied into the image and attached to its digest, the
//! labels are what let a chart find it without pulling a layer, and the `LABEL` block is those
//! labels in the form a Dockerfile can carry. Generate them from one run of this program in one
//! build — that is the only thing that makes it impossible for them to disagree — and check the
//! built image against the labels, because a hand-pasted `LABEL` block with nothing checking it is
//! exactly the failure the labels exist to rule out. The `rust/config-contract` action is that
//! check, and the Dockerfile drift gate beside it.
//!
//! Nothing below reads the environment, so the output is the same on a developer's machine as on
//! a runner where none of the variables it describes exist. `--version`, `--revision` and
//! `--created` are the three things that legitimately differ between builds of one source tree,
//! and they are flags for that reason: omitted, `--format contract` is byte-reproducible, which
//! is what lets `docs/config.contract.json` be committed and diffed in review.

use std::process::ExitCode;

use s3_bucket_perma_link::config;
use terrace_config::schema::JsonSchema;
use terrace_config::schema::cli::Cli;

/// The `$id` the generated JSON Schema carries.
const SCHEMA_ID: &str = "https://github.com/TimSchoenle/s3-bucket-perma-link/config.schema.json";

fn main() -> ExitCode {
    let schema = match config::schema() {
        Ok(schema) => schema,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    Cli::new(config::app())
        .json_schema(
            JsonSchema::new()
                .title("s3-bucket-perma-link configuration")
                .id(SCHEMA_ID),
        )
        // Declared in `src/config.rs` rather than here, so that `config::contract` — which the
        // unit tests build — and this generator cannot describe different external surfaces.
        .contract_with(&|builder| builder.external(config::external()))
        .main(schema)
}
