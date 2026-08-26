//! The Sentry client, and the three sinks that feed it.
//!
//! Off unless `telemetry.sentry.enabled` is set, and then only with a DSN: a client that reports
//! nowhere is worse than no client, because it is discovered during the incident it was
//! installed for. The combination is refused at boot rather than logged and stepped over.
//!
//! All three sinks share the one client [`init`] installs:
//! - **`tracing`** — [`tracing_layer`] turns the service's own records into issues and
//!   breadcrumbs under the thresholds in [`SentryConfig`], and its spans into Sentry spans.
//! - **panics** — the SDK's own hook, added by `sentry::init`.
//! - **HTTP** — [`middleware`], mounted by [`crate::server::Server`]: a hub per request, and
//!   optionally a transaction per request named by the matched route.
//!
//! The extern crate is always spelled `::sentry`; the bare path is ambiguous with this module.

use std::sync::OnceLock;
use std::time::Duration;

use ::sentry::integrations::tracing::{EventFilter, SentryLayer, default_span_filter};
use actix_web::middleware::Condition;
use secrecy::ExposeSecret;
use tracing::Level;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::registry::LookupSpan;

use crate::config::{SentryConfig, SentryLevel};
use crate::error::Error;

/// What [`crate::server::Server`] mounts, decided once at boot.
///
/// Process-global because the client it describes is: `sentry::init` binds one client to
/// `Hub::main()` for the lifetime of the process, while the listener around it is rebuilt on
/// every configuration reload. Carrying the flag on `Server` instead would make a
/// per-generation copy of a value that cannot change between generations.
///
/// Unset until [`init`] runs, which `main` does before it builds a listener.
static HTTP: OnceLock<HttpOptions> = OnceLock::new();

/// The two independent halves of the HTTP integration.
#[derive(Debug, Clone, Copy)]
struct HttpOptions {
    /// A client is bound, so requests get their own hub and their request metadata.
    active: bool,
    /// Additionally start one transaction per request. Whether that transaction is *kept* is
    /// the sampler's decision, not this one.
    transactions: bool,
}

/// Install the process-wide Sentry client, or nothing when it is switched off.
///
/// Returns the guard that flushes queued events on drop. The caller must hold it for the
/// lifetime of the process — [`crate::telemetry::TelemetryGuard`] is what does that.
///
/// # Errors
/// [`Error::Sentry`] when `enabled` is set without a DSN, when the DSN does not parse, or when
/// a sample rate falls outside `0.0..=1.0`. All three are configuration mistakes whose only
/// other outcome is a service that silently reports nothing.
pub(crate) fn init(cfg: &SentryConfig) -> crate::Result<Option<::sentry::ClientInitGuard>> {
    if !cfg.enabled() {
        record_http(HttpOptions {
            active: false,
            transactions: false,
        });
        return Ok(None);
    }

    // Empty is absent, not a value. `S3_PERMA_LINK_TELEMETRY__SENTRY__DSN=""` is what an
    // unfilled chart value or a compose pass-through produces, and it has to land on the
    // message below rather than on the parse error, which would send an operator looking at
    // their URL.
    let dsn = cfg
        .dsn()
        .as_ref()
        .map(|dsn| dsn.expose_secret().trim())
        .filter(|dsn| !dsn.is_empty())
        .ok_or_else(|| {
            Error::Sentry(
                "telemetry.sentry.enabled is set but telemetry.sentry.dsn is empty; nothing \
                 would be reported. Set the DSN or turn the section off."
                    .to_owned(),
            )
        })?;

    // Parsed here rather than through `ClientOptions::dsn`, which panics on a malformed value.
    // The message deliberately does not quote the DSN: it is a credential, and this reaches the
    // log stream.
    let dsn = dsn.parse::<::sentry::types::Dsn>().map_err(|e| {
        Error::Sentry(format!(
            "telemetry.sentry.dsn is not a valid Sentry DSN ({e}); expected \
             https://<key>@<host>/<project>"
        ))
    })?;

    check_rate("sample_rate", *cfg.sample_rate())?;
    check_rate("traces_sample_rate", *cfg.traces_sample_rate())?;

    let mut options = ::sentry::ClientOptions::new()
        .debug(*cfg.debug())
        .sample_rate(*cfg.sample_rate())
        .traces_sample_rate(*cfg.traces_sample_rate())
        .max_breadcrumbs(*cfg.max_breadcrumbs())
        .attach_stacktrace(*cfg.attach_stacktrace())
        .send_default_pii(*cfg.send_default_pii())
        .shutdown_timeout(Duration::from_secs(*cfg.shutdown_timeout_secs()))
        .environment(environment(cfg))
        .release(release(cfg))
        // Marks our own frames as application code, so a stack trace opens on the handler
        // rather than on an actix internal. The crate name as a linker sees it, hence the
        // underscore.
        .in_app_include(vec!["s3_bucket_perma_link"]);
    options.dsn = Some(dsn);
    if let Some(server_name) = cfg.server_name().clone() {
        options = options.server_name(server_name);
    }

    // Every field `apply_defaults` would otherwise fill from `SENTRY_DSN`, `SENTRY_RELEASE` or
    // `SENTRY_ENVIRONMENT` is set above. That is the point rather than a coincidence: those
    // three are a second configuration channel that bypasses the layered loader and its
    // shadow-key rejection, and an already-set field is one they cannot reach.
    let guard = ::sentry::init(options);

    record_http(HttpOptions {
        active: true,
        transactions: *cfg.http_transactions(),
    });

    Ok(Some(guard))
}

/// The `tracing` layer feeding the client, or `None` when Sentry is off.
///
/// Carries its own [`LevelFilter`] rather than sitting under the console layer's: the two sinks
/// answer to different keys, so `telemetry.log_level = "warn"` does not quietly stop
/// `breadcrumb_level = "info"` from collecting anything. The filter is the more verbose of the
/// two Sentry thresholds, which is also what keeps the registry from evaluating records neither
/// sink wants.
pub(crate) fn tracing_layer<S>(cfg: &SentryConfig) -> Option<impl Layer<S>>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    if !cfg.enabled() {
        return None;
    }

    let capture = *cfg.capture_level();
    let breadcrumb = *cfg.breadcrumb_level();

    let mut layer: SentryLayer<S> = ::sentry::integrations::tracing::layer()
        .event_filter(move |metadata| {
            let level = *metadata.level();
            if accepts(capture, level) {
                EventFilter::Event
            } else if accepts(breadcrumb, level) {
                EventFilter::Breadcrumb
            } else {
                EventFilter::Ignore
            }
        })
        // Not additionally gated on `traces_sample_rate`. Whether a span is *kept* is the
        // sampler's decision, and it is the one that can honour a trace this service was handed
        // rather than started: gating span creation here would cut an inbound trace at this hop
        // for a deployment that deliberately starts none of its own.
        .span_filter(default_span_filter);

    if *cfg.span_attributes() {
        layer = layer.enable_span_attributes();
    }

    Some(layer.with_filter(level_filter(capture).max(level_filter(breadcrumb))))
}

/// The per-request middleware, inert when Sentry is off.
///
/// Read off the process-global recorded by [`init`] rather than passed down from the
/// configuration, because that is what it describes — one client per process, against a
/// listener that is rebuilt on every reload.
///
/// The hub half is not optional decoration: without a hub per request, breadcrumbs from
/// concurrently served downloads all land on the main hub and every issue arrives with a trail
/// belonging to whoever else was in flight. The transaction half is
/// `telemetry.sentry.http_transactions`, and names the transaction after the matched route
/// (`GET /{path}`) rather than the URI, so one permanent link does not become one transaction
/// name.
///
/// A [`Condition`] rather than an `Option`, because actix decides its middleware stack in the
/// type system: there is no `Option<Transform>` to hand `App::wrap`.
#[must_use]
pub fn middleware() -> Condition<::sentry::integrations::actix::Sentry> {
    let options = HTTP.get().copied().unwrap_or(HttpOptions {
        active: false,
        transactions: false,
    });

    // Built even when inert — `Condition` needs a transform to hold either way — but with no
    // client bound the middleware has nothing to report onto, and `Condition::new(false, …)`
    // never calls it. `capture_server_errors` stays at its default: a 5xx that never reached a
    // `tracing` record is otherwise invisible to the layer above.
    let sentry = ::sentry::integrations::actix::Sentry::builder()
        .start_transaction(options.transactions)
        .finish();

    Condition::new(options.active, sentry)
}

/// The environment tag, defaulted here rather than left to the SDK.
///
/// The SDK's own fallback is this same rule, but reading `SENTRY_ENVIRONMENT` first — and that
/// variable is the back channel `external()` no longer has to declare. Resolving it here closes
/// it.
fn environment(cfg: &SentryConfig) -> String {
    cfg.environment().clone().unwrap_or_else(|| {
        if cfg!(debug_assertions) {
            "development".to_owned()
        } else {
            "production".to_owned()
        }
    })
}

/// The release tag: the image's name and the version it was cut from.
///
/// `sentry::release_name!` would give the crate name with an underscore; the hyphenated form is
/// what the image, the repository and every other artefact call this service.
fn release(cfg: &SentryConfig) -> String {
    cfg.release()
        .clone()
        .unwrap_or_else(|| format!("{}@{}", crate::config::APP_NAME, env!("CARGO_PKG_VERSION")))
}

/// Whether a record at `level` is at least as severe as `threshold`.
///
/// [`tracing::Level`] orders `ERROR` lowest, so "at least as severe" is `<=`.
fn accepts(threshold: SentryLevel, level: Level) -> bool {
    let threshold = match threshold {
        SentryLevel::Off => return false,
        SentryLevel::Error => Level::ERROR,
        SentryLevel::Warn => Level::WARN,
        SentryLevel::Info => Level::INFO,
        SentryLevel::Debug => Level::DEBUG,
        SentryLevel::Trace => Level::TRACE,
    };
    level <= threshold
}

/// The subscriber-side spelling of a threshold, so the registry can skip a record before it is
/// built rather than after [`accepts`] has rejected it.
fn level_filter(threshold: SentryLevel) -> LevelFilter {
    match threshold {
        SentryLevel::Off => LevelFilter::OFF,
        SentryLevel::Error => LevelFilter::ERROR,
        SentryLevel::Warn => LevelFilter::WARN,
        SentryLevel::Info => LevelFilter::INFO,
        SentryLevel::Debug => LevelFilter::DEBUG,
        SentryLevel::Trace => LevelFilter::TRACE,
    }
}

fn check_rate(key: &str, rate: f32) -> crate::Result<()> {
    if (0.0..=1.0).contains(&rate) {
        Ok(())
    } else {
        Err(Error::Sentry(format!(
            "telemetry.sentry.{key} must be between 0.0 and 1.0, got {rate}"
        )))
    }
}

/// First writer wins, matching the client itself: a second `init` in one process is a test
/// harness, not a reconfiguration.
fn record_http(options: HttpOptions) {
    let _ = HTTP.set(options);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`tracing::Level`] sorts `ERROR` *below* `TRACE`, so a severity threshold reads as `<=`
    /// and not `>=`. Inverting it turns `capture_level = "error"` into "capture everything",
    /// which arrives as a bill rather than as a compile error.
    #[test]
    fn a_threshold_accepts_only_levels_at_least_as_severe() {
        assert!(accepts(SentryLevel::Error, Level::ERROR));
        assert!(!accepts(SentryLevel::Error, Level::WARN));
        assert!(!accepts(SentryLevel::Error, Level::TRACE));

        assert!(accepts(SentryLevel::Info, Level::ERROR));
        assert!(accepts(SentryLevel::Info, Level::WARN));
        assert!(accepts(SentryLevel::Info, Level::INFO));
        assert!(!accepts(SentryLevel::Info, Level::DEBUG));

        for level in [
            Level::ERROR,
            Level::WARN,
            Level::INFO,
            Level::DEBUG,
            Level::TRACE,
        ] {
            assert!(!accepts(SentryLevel::Off, level));
            assert!(accepts(SentryLevel::Trace, level));
        }
    }

    /// The layer's own filter has to admit everything *either* threshold wants. Taking the
    /// capture level alone — the tempting simplification, since it is the more severe of the
    /// two by default — would drop every breadcrumb before [`accepts`] ever saw it.
    #[test]
    fn the_layer_filter_is_the_more_verbose_of_the_two_thresholds() {
        let filter = level_filter(SentryLevel::Error).max(level_filter(SentryLevel::Info));
        assert_eq!(filter, LevelFilter::INFO);

        let filter = level_filter(SentryLevel::Debug).max(level_filter(SentryLevel::Off));
        assert_eq!(filter, LevelFilter::DEBUG);

        // Both sinks silent is a layer that is asked for nothing at all.
        let filter = level_filter(SentryLevel::Off).max(level_filter(SentryLevel::Off));
        assert_eq!(filter, LevelFilter::OFF);
    }

    #[test]
    fn a_sample_rate_outside_the_unit_interval_is_refused() {
        assert!(check_rate("sample_rate", 0.0).is_ok());
        assert!(check_rate("sample_rate", 1.0).is_ok());
        assert!(check_rate("sample_rate", -0.1).is_err());
        assert!(check_rate("sample_rate", 1.1).is_err());
    }

    /// The disabled path must install no client at all — not a client with an empty DSN, which
    /// still starts a transport thread and still queues events.
    #[test]
    fn disabled_installs_no_client() {
        let cfg = SentryConfig::default();
        assert!(!cfg.enabled());
        assert!(init(&cfg).expect("the disabled path cannot fail").is_none());
        assert!(tracing_layer::<tracing_subscriber::Registry>(&cfg).is_none());
    }

    /// `enabled` without a DSN reports nowhere, so it fails the boot instead of starting a
    /// service its operator believes is reporting.
    /// The block as the loader would hand it over, so these tests exercise the same
    /// deserialisation `Config` does rather than a hand-built struct that could drift from it.
    fn config(json: serde_json::Value) -> SentryConfig {
        serde_json::from_value(json).expect("the block deserialises from its own keys")
    }

    /// `enabled` without a DSN reports nowhere, so it fails the boot instead of starting a
    /// service its operator believes is reporting.
    #[test]
    fn enabled_without_a_dsn_is_a_boot_failure() {
        let cfg = config(serde_json::json!({ "enabled": true }));
        // `expect_err` is unavailable here: `ClientInitGuard` implements no `Debug`, so the
        // success arm cannot be printed.
        let Err(error) = init(&cfg) else {
            panic!("a client with no DSN must not be installed")
        };
        assert!(error.to_string().contains("dsn"), "{error}");
    }

    /// A pass-through that resolved to nothing — `…__SENTRY__DSN=""` from a compose file, an
    /// unfilled chart value — must read as *absent* rather than as a DSN that fails to parse.
    /// The two produce very different messages and only one sends the operator to the right
    /// place.
    #[test]
    fn an_empty_dsn_reads_as_absent_rather_than_malformed() {
        let cfg = config(serde_json::json!({ "enabled": true, "dsn": "   " }));
        // `expect_err` is unavailable here: `ClientInitGuard` implements no `Debug`, so the
        // success arm cannot be printed.
        let Err(error) = init(&cfg) else {
            panic!("a blank DSN reports nowhere either")
        };
        assert!(error.to_string().contains("is empty"), "{error}");
    }

    /// A DSN that is present but not a DSN is the other half of the same claim, and the one
    /// `ClientOptions::dsn` would turn into a panic rather than an error.
    #[test]
    fn a_malformed_dsn_is_an_error_rather_than_a_panic() {
        let cfg = config(serde_json::json!({ "enabled": true, "dsn": "not-a-dsn" }));
        // `expect_err` is unavailable here: `ClientInitGuard` implements no `Debug`, so the
        // success arm cannot be printed.
        let Err(error) = init(&cfg) else {
            panic!("a malformed DSN must not reach ClientOptions")
        };
        assert!(
            error.to_string().contains("not a valid Sentry DSN"),
            "{error}"
        );
        // The credential must not be echoed into the log stream along with the complaint.
        assert!(!error.to_string().contains("not-a-dsn"), "{error}");
    }

    /// The two rates a mistyped chart value lands on. `1` is a fraction, not a percentage, and
    /// `100` has to be refused rather than clamped: clamping would make the mistake invisible.
    #[test]
    fn a_configured_rate_outside_the_unit_interval_fails_the_boot() {
        let cfg = config(serde_json::json!({
            "enabled": true,
            "dsn": "https://key@sentry.example/1",
            "traces_sample_rate": 100.0,
        }));
        // `expect_err` is unavailable here: `ClientInitGuard` implements no `Debug`, so the
        // success arm cannot be printed.
        let Err(error) = init(&cfg) else {
            panic!("a rate of 100 is a percentage, not a fraction")
        };
        assert!(error.to_string().contains("traces_sample_rate"), "{error}");
    }
}
