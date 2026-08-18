//! Emit the variable payload for `.github/templates/README.md.hbs` as strict JSON on stdout.
//!
//! `README.md` is rendered from that template by `.github/workflows/docs.yml`, so every value in
//! it that is derived from something else in the repository is derived *here* rather than typed
//! into the README and left to rot. Two kinds of value qualify:
//!
//! - **The configuration reference.** Both tables come off `Config` through `config::schema`,
//!   which means a key added to that type without a `///` comment shows an empty Purpose cell in
//!   the pull request that adds it, and a key removed from it leaves the README on the same
//!   commit.
//! - **The spellings the surrounding prose quotes.** The prefix, `S3_PERMA_LINK_CONFIG` and
//!   `S3_PERMA_LINK_SECRETS_DIR` come from the loader itself, so prose naming a variable the
//!   service does not read is not expressible.
//!
//! The repository coordinates below it are the third kind: a constant with exactly one
//! definition, rather than the same string typed into nine badge URLs.
//!
//! Run it yourself to see what CI will render with:
//!
//! ```text
//! cargo run --example readme-variables
//! ```
//!
//! Output is one line, because the workflow step carries it through `$GITHUB_OUTPUT`, where a
//! value is one line by definition.

use std::process::ExitCode;

use s3_bucket_perma_link::config;
use serde_json::json;
use terrace_config::schema::Column;

/// `owner/repo`, as every badge and link in the README spells it.
const REPOSITORY: &str = "TimSchoenle/s3-bucket-perma-link";

/// The branch the README's permalinks point at.
const BRANCH: &str = "master";

/// The workflow whose status the build badge shows. A file name, not a display name — the badge
/// URL takes the path, and getting it wrong renders a badge that reads "no status".
const BUILD_WORKFLOW: &str = "build.yaml";

/// The published image. Not derivable from the manifest: nothing in `Cargo.toml` names it.
const DOCKER_IMAGE: &str = "timmi6790/s3-bucket-perma-link";

fn main() -> ExitCode {
    match variables() {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("readme-variables: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The payload, as one line of strict JSON.
///
/// Fails if the schema cannot be built or the payload cannot be serialised. Neither is reachable
/// while `cargo test` passes, and both are worth failing the documentation job over rather than
/// rendering a README around a hole.
fn variables() -> Result<String, Box<dyn std::error::Error>> {
    let terrace = config::terrace();
    let schema = config::schema()?;

    let payload = json!({
        "repo": REPOSITORY,
        "branch": BRANCH,
        "build_workflow": BUILD_WORKFLOW,
        "docker_image": DOCKER_IMAGE,
        "prefix": terrace.dialect().prefix(),
        "config_var": terrace.config_var_name(),
        "secrets_dir_var": terrace.secrets_dir_var_name(),
        "indirection_suffix": terrace.dialect().indirection_suffix(),
        // The two tables separately rather than through `to_markdown`: the README puts a
        // paragraph between them, and welding them together here would mean cutting them apart
        // in the template.
        "loader_table": schema.to_markdown_loader(),
        "keys_table": schema.to_markdown_keys(Column::DEFAULT),
    });

    Ok(serde_json::to_string(&payload)?)
}
