//! The `tracing` -> Sentry wiring, with a client actually bound.
//!
//! Its own test binary, and one test in it, because both halves are process-global: a `tracing`
//! subscriber can be installed once per process and so can a Sentry client. The unit tests in
//! `src/telemetry/sentry.rs` cover everything that can be decided without one — the severity
//! thresholds, the layer filter, and the four ways a configured DSN is refused.
//!
//! What is left needs the real thing installed: that `telemetry::init` reaches the end with a
//! usable DSN, and that the layer it installs actually turns a `tracing` span into a Sentry
//! span. Nothing here sends anything — the DSN points at a loopback address that answers
//! nothing, and `shutdown_timeout_secs = 0` means the guard's flush on drop returns at once.

#![cfg(feature = "sentry")]

use s3_bucket_perma_link::config::TelemetryConfig;
use s3_bucket_perma_link::telemetry;

/// The block as the loader would hand it over, so this exercises the same deserialisation the
/// service boots through rather than a hand-built struct that could drift from it.
fn telemetry_config() -> TelemetryConfig {
    serde_json::from_value(serde_json::json!({
        "log_level": "info",
        "sentry": {
            "enabled": true,
            // Parses, resolves, and answers nothing.
            "dsn": "https://0123456789abcdef@127.0.0.1/1",
            // Spans have to be started for this to be about anything.
            "traces_sample_rate": 1.0,
            // Nothing is captured here, so there is nothing to drain — and a non-zero value
            // would be paid on the guard's drop at the end of the test.
            "shutdown_timeout_secs": 0,
        },
    }))
    .expect("the telemetry block deserialises from its own keys")
}

/// The trace-continuation headers for whatever span is in scope: `sentry-trace`, and whatever
/// else the SDK adds to that set later.
fn trace() -> Option<String> {
    let mut headers = Vec::new();
    // `configure_scope` returns `()`, so the iterator has to be drained into a binding the
    // closure captures rather than returned through it.
    sentry::configure_scope(|scope| headers.extend(scope.iter_trace_propagation_headers()));
    headers
        .into_iter()
        .find(|(name, _)| *name == "sentry-trace")
        .map(|(_, value)| value)
}

/// A configured DSN installs a client, and the layer it installs puts a `tracing` span into a
/// Sentry trace of its own.
///
/// The second half is the claim worth a test binary. A bound client always reports *some*
/// propagation context, so "the headers are non-empty" would pass with the layer removed
/// entirely; what distinguishes a working layer is that entering a span *changes* the trace the
/// process would hand on. That is the whole mechanism behind a transaction, a breadcrumb trail
/// scoped to one download, and a stack trace that arrives attached to the request that caused
/// it.
#[test]
fn an_installed_layer_puts_a_tracing_span_into_its_own_sentry_trace() {
    let guard = telemetry::init(&telemetry_config()).expect("a usable DSN installs a client");

    let outside = trace().expect("a bound client always reports a propagation context");

    let inside = {
        let span = tracing::info_span!("download");
        let _entered = span.enter();
        trace().expect("a span under the layer is a trace that can be handed on")
    };

    assert_ne!(
        inside, outside,
        "entering a span must start a Sentry span of its own; an unchanged trace means the \
         layer is not installed, or its span filter rejected an `info` span"
    );

    // Explicit rather than left to the end of the test: dropping the guard is what closes the
    // client, and doing it here says that this test owns the process-global it installed.
    drop(guard);
}
