//! Emit the half of the README render payload that only this crate can produce, as strict JSON
//! on stdout.
//!
//! `README.md` is rendered from `.github/templates/README.md.hbs` by
//! `.github/workflows/docs.yml`. Most of what that template interpolates is derived from files
//! in the checkout by `TimSchoenle/actions/actions/common/readme-variables` — the repository
//! coordinates, the release read off `Cargo.toml`, the documentation index. This program
//! supplies the rest, and the workflow hands it over as that action's `extra` input, where it is
//! deep-merged over the derived payload.
//!
//! Two kinds of value qualify:
//!
//! - **The configuration reference.** Both tables come off `Config` through `config::schema`,
//!   which means a key added to that type without a `///` comment shows an empty Purpose cell in
//!   the pull request that adds it, and a key removed from it leaves the README on the same
//!   commit.
//! - **The spellings the surrounding prose quotes.** The prefix, `S3_PERMA_LINK_CONFIG` and
//!   `S3_PERMA_LINK_SECRETS_DIR` come from the loader itself, so prose naming a variable the
//!   service does not read is not expressible.
//!
//! Below them sit the two coordinates no file in this repository declares: the workflow behind
//! the build badge, and the image this releases to.
//!
//! Nothing here emits `repo`, `branch`, `release` or `docs`. The merge is a deep merge over the
//! derived payload, so a key spelled the same way replaces what the action worked out. `repo`
//! there is an object the template reads fields out of, and a slug emitted under that name would
//! take every one of them with it.
//!
//! Run it yourself to see what CI will merge in:
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

/// The `extra` half of the payload, as one line of strict JSON.
///
/// Fails if the schema cannot be built or the payload cannot be serialised. Neither is reachable
/// while `cargo test` passes, and both are worth failing the documentation job over rather than
/// rendering a README around a hole.
fn variables() -> Result<String, Box<dyn std::error::Error>> {
    let terrace = config::terrace();
    let schema = config::schema()?;

    let payload = json!({
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
