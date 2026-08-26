//! Installing the log subscriber, and the optional Sentry client that shares its record stream.
//!
//! Process-global and installed once, before the reload supervisor is reached: a `tracing`
//! subscriber cannot be replaced on a running process and neither can a Sentry client. That is
//! why `telemetry.*` is the one configuration block a reload does not apply, and why [`init`]
//! is called from `main` rather than from the per-generation `serve`.
//!
//! The two sinks are filtered **independently**. The console layer takes
//! `telemetry.log_level`; the Sentry layer takes its own thresholds from
//! `telemetry.sentry.capture_level` and `telemetry.sentry.breadcrumb_level`. Sharing one global
//! filter would be less code and a worse deployment: tightening the console to `warn` would
//! silently empty every breadcrumb trail, which is the trail the next issue arrives with.

#[cfg(feature = "sentry")]
pub mod sentry;

use crate::Result;
use crate::config::TelemetryConfig;
use crate::error::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, filter};

/// Keeps the Sentry client alive, and flushes what it has queued on drop.
///
/// Returned rather than leaked into a static because a static is never dropped, and the drop is
/// the point: it is what gets the last events of a shutting-down replica out of the process,
/// bounded by `telemetry.sentry.shutdown_timeout_secs`. Bind it for the lifetime of `main`
/// (`let _telemetry = …`); `let _ = …` drops it immediately and closes the client before the
/// service has served anything.
///
/// An empty struct in a build without the `sentry` feature, so `main` reads the same either way.
#[must_use = "dropping the guard closes the Sentry client and stops reporting"]
pub struct TelemetryGuard {
    #[cfg(feature = "sentry")]
    sentry: Option<::sentry::ClientInitGuard>,
}

/// Hand-written because `ClientInitGuard` implements no [`Debug`] of its own, and because the
/// only thing worth printing about this is the one bit the field carries: whether a client is
/// bound. The guard also derefs to the client, so a derive would be reaching for a credential.
impl std::fmt::Debug for TelemetryGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "sentry")]
        let sentry = self.sentry.is_some();
        #[cfg(not(feature = "sentry"))]
        let sentry = false;

        f.debug_struct("TelemetryGuard")
            .field("sentry", &sentry)
            .finish()
    }
}

/// Install the global `tracing` subscriber, and the Sentry client when one is configured.
///
/// # Errors
/// Returns [`Error::Logger`] if `telemetry.log_level` does not name a level, or
/// [`Error::Tracing`] if a subscriber is already installed.
#[cfg_attr(
    feature = "sentry",
    doc = "Returns [`Error::Sentry`] if `telemetry.sentry` is switched on but unusable."
)]
pub fn init(telemetry: &TelemetryConfig) -> Result<TelemetryGuard> {
    let level = telemetry.level()?;

    // Before the subscriber, on purpose: the layer below reports onto the client this installs,
    // and the SDK's panic hook should already be in place for anything the subscriber build
    // itself does.
    #[cfg(feature = "sentry")]
    let guard = sentry::init(telemetry.sentry())?;

    let registry = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter::LevelFilter::from_level(level)));

    #[cfg(feature = "sentry")]
    let registry = registry.with(sentry::tracing_layer(telemetry.sentry()));

    registry
        .try_init()
        .map_err(|e| Error::Tracing(e.to_string()))?;

    // After `try_init`, not beside `sentry::init`: a record emitted before the subscriber exists
    // goes nowhere, and "is Sentry actually on in this pod" is the first question an operator
    // asks of a service that is not reporting.
    #[cfg(feature = "sentry")]
    if guard.is_some() {
        let sentry = telemetry.sentry();
        info!(
            traces_sample_rate = sentry.traces_sample_rate(),
            send_default_pii = sentry.send_default_pii(),
            http_transactions = sentry.http_transactions(),
            "Sentry reporting enabled"
        );
    } else {
        info!("Sentry is not enabled; no error reporting or tracing egress");
    }

    Ok(TelemetryGuard {
        #[cfg(feature = "sentry")]
        sentry: guard,
    })
}
