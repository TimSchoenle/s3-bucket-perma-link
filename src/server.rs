use crate::Result;
use crate::data::DownloadData;
use crate::routes::{download, health_check};
use actix_web::{App, HttpServer, web};
use derive_new::new;
use tokio_util::sync::CancellationToken;
use tracing_actix_web::TracingLogger;

#[derive(new)]
pub struct Server {
    host: String,
    port: u16,
}

impl Server {
    /// Serve until `shutdown` is cancelled.
    ///
    /// Returns only once the listener has released the address and in-flight requests have
    /// drained. The reload supervisor relies on that: it builds the replacement only after this
    /// future returns, and a replacement that raced this one would fail to bind.
    pub async fn run_until_stopped(
        &self,
        download_data: DownloadData,
        shutdown: CancellationToken,
    ) -> Result<()> {
        info!("Starting server on {}:{}", self.host, self.port,);

        let download_data = web::Data::new(download_data);
        let server = HttpServer::new(move || {
            App::new()
                .wrap(TracingLogger::default())
                .app_data(download_data.clone())
                .configure(health_check::get_config)
                .configure(download::get_config)
        })
        // The process lifecycle belongs to `shutdown`: actix's own handler would stop this
        // listener without telling the supervisor, which would then rebuild it and leave a
        // service that ignored its own `SIGTERM`.
        .disable_signals()
        .bind((self.host.clone(), self.port))?
        .run();

        let handle = server.handle();
        let stopper = tokio::spawn(async move {
            shutdown.cancelled().await;
            // Graceful: in-flight downloads finish before the address is released.
            handle.stop(true).await;
        });

        let outcome = server.await;
        // The server can also stop on its own; the waiting task would otherwise outlive it and
        // hold the token clone until the process exits.
        stopper.abort();
        outcome?;

        Ok(())
    }
}
