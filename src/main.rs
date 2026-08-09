use s3_bucket_perma_link::config::{Config, TelemetryConfig};
use s3_bucket_perma_link::data::DownloadData;
use secrecy::ExposeSecret;
use sentry::ClientInitGuard;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, filter};

use s3_bucket_perma_link::error::Error;
use s3_bucket_perma_link::server::Server;
use s3_bucket_perma_link::{Result, config, shutdown};

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() -> Result<()> {
    // Before the subscriber exists, because it is the configuration that says what the
    // subscriber should be. A failure here is returned from `main` and printed by the runtime.
    let boot = config::load_watched::<Config>()?;

    // Both are process-global and installed once, which is why `telemetry.*` is the one block
    // a configuration reload cannot apply.
    setup_tracing(boot.value.telemetry())?;
    let _sentry = setup_sentry(boot.value.telemetry());

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

/// Build and run everything a configuration change rebuilds: the bucket clients, the routing
/// table they are looked up through, and the listener.
///
/// Returns when `shutdown` is cancelled — by the OS signal, or by the supervisor because the
/// configuration changed and this runtime is being replaced.
async fn serve(config: Arc<Config>, shutdown: CancellationToken) -> Result<()> {
    let buckets = config.s3().create_buckets(config.bucket().entries())?;
    let download_data = DownloadData::new(buckets, config.bucket().entries().clone());

    let server = Server::new(config.server().host().to_string(), *config.server().port());
    server.run_until_stopped(download_data, shutdown).await
}

fn setup_tracing(telemetry: &TelemetryConfig) -> Result<()> {
    let level = telemetry.level()?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter::LevelFilter::from_level(level)))
        .init();

    Ok(())
}

fn setup_sentry(telemetry: &TelemetryConfig) -> Option<ClientInitGuard> {
    let Some(dsn) = telemetry.sentry_dsn() else {
        info!("No Sentry DSN configured, skipping Sentry setup");
        return None;
    };

    Some(sentry::init((
        dsn.expose_secret(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            attach_stacktrace: true,
            ..Default::default()
        },
    )))
}
