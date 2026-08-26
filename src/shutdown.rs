//! Turning the operator's stop signal into one token every task can observe.
//!
//! The listener no longer installs its own handler, because
//! [`Server::run_until_stopped`](crate::server::Server::run_until_stopped) disables actix's: the
//! reload supervisor stops and rebuilds it, so the signal has to reach the supervisor rather
//! than the listener it happens to be running.

use tokio_util::sync::CancellationToken;

/// Spawns the signal handler and returns the token it cancels.
///
/// `SIGTERM` as well as `SIGINT`: `SIGTERM` is what a container runtime sends, and a service
/// that only watches `SIGINT` is killed rather than drained on every rollout.
#[must_use]
pub fn install() -> CancellationToken {
    let token = CancellationToken::new();
    let signalled = token.clone();

    tokio::spawn(async move {
        wait_for_signal().await;
        info!("Shutdown signal received, stopping");
        signalled.cancel();
    });

    token
}

#[cfg(unix)]
async fn wait_for_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    // A failure here would leave the process unstoppable except by `SIGKILL`, which is worth
    // crashing over at startup rather than discovering during a rollout.
    let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler can be installed");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_signal() {
    // `SIGTERM` has no Windows equivalent; `ctrl_c` also covers the console close events tokio
    // maps onto it.
    let _ = tokio::signal::ctrl_c().await;
}
