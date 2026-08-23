//! The process: load the configuration, install the two things that outlive a reload, then hand
//! the rest to the supervisor.
//!
//! Everything a configuration change can rebuild sits behind `serve`, which the supervisor calls
//! again with the new configuration and the token that stops the old one. The tracing subscriber,
//! and the Sentry client a `sentry` build carries, are installed here instead, before the
//! supervisor exists, because both are process-global and neither can be replaced on a running
//! process.
//!
//! The exit path is the same for a failed boot and a failed reload: the error is returned from
//! `main` and printed by the runtime. There is no fallback configuration, so a process that
//! cannot read its own settings binds nothing.

use s3_bucket_perma_link::config::Config;
use s3_bucket_perma_link::data::DownloadData;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use s3_bucket_perma_link::error::Error;
use s3_bucket_perma_link::server::Server;
use s3_bucket_perma_link::{Result, config, shutdown, telemetry};

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() -> Result<()> {
    // Before the subscriber exists, because it is the configuration that says what the
    // subscriber should be. A failure here is returned from `main` and printed by the runtime.
    let boot = config::load_watched::<Config>()?;

    // The subscriber and the Sentry client are process-global and installed once, which is why
    // `telemetry.*` is the one block a configuration reload cannot apply.
    //
    // Bound for the whole of `main`, not dropped on the spot: the guard is what flushes queued
    // Sentry events on the way out, so `let _ = …` here would close the client before the
    // service had served anything.
    let _telemetry = telemetry::init(boot.value.telemetry())?;

    let shutdown = shutdown::install();

    // Rebuilds the runtime whenever a mounted configuration file changes, so a rotated S3
    // credential is picked up without restarting the pod. A reload that fails to load, or that
    // resolves to what is already running, leaves the running service exactly as it is.
    terrace_config::reload::run(
        (boot.value, boot.sources),
        &shutdown,
        || {
            config::load_watched::<Config>()
                .map(|loaded| (loaded.value, loaded.sources))
                .map_err(Error::from)
        },
        serve,
    )
    .await
}

/// Runs one generation of the service: the bucket clients, the routing table they are looked
/// up through, and the listener that serves from it.
///
/// Returns when `shutdown` is cancelled — by the OS signal, or by the supervisor because the
/// configuration changed and this runtime is being replaced.
async fn serve(config: Arc<Config>, shutdown: CancellationToken) -> Result<()> {
    log_config_sources();

    let buckets = config.s3().create_buckets(config.bucket().entries())?;
    let download_data = DownloadData::new(buckets, config.bucket().entries().clone());

    let server = Server::new(config.server().host().clone(), *config.server().port());
    server.run_until_stopped(download_data, shutdown).await
}

/// Logs which layer supplied each configuration key.
///
/// Here rather than at boot because this runs once per generation: after a reload the report
/// describes the layers *that* load saw, which is the one moment the question "where did this
/// value come from" is being asked about something that just changed.
///
/// The report carries no configuration value — the keys and their sources, never the contents —
/// so there is nothing in it to redact. A failure to assemble it is not a failure to serve: the
/// configuration it describes has already loaded, so it is logged and stepped over.
fn log_config_sources() {
    match config::explain() {
        Ok(explanation) => info!("Configuration sources:\n{explanation}"),
        Err(error) => warn!("Could not explain the configuration sources: {error}"),
    }
}
