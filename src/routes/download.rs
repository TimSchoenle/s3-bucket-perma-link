use crate::data::DownloadData;
use actix_web::{HttpResponse, web};
use tokio_stream::StreamExt;

/// Registers the catch-all download route.
///
/// `{tail:.*}` matches every path, the empty one included, so this has to be the last
/// registration on the app.
pub fn get_config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("{tail:.*}").route(web::get().to(download)));
}

/// Streams the object bound to `path`.
///
/// 404 when `path` names no entry, decided before any S3 call, so the bucket can be neither
/// listed nor probed key by key. 500 when the object cannot be opened, with the store's reason
/// logged rather than returned. The body is forwarded chunk by chunk as the store produces it,
/// so the response carries no `Content-Length` and the object is never held whole. A failure
/// after the first chunk cannot change the status, so the client sees a short body under a 200.
async fn download(
    path: web::Path<String>,
    download_data: web::Data<DownloadData>,
) -> core::result::Result<HttpResponse, actix_web::Error> {
    info!("Received request for path: {}", path);
    match download_data.get_entry(&path) {
        Some(bucket) => {
            info!("Valid path request!");

            if let Some(bucket_client) = download_data.buckets().get(path.as_str()) {
                match bucket_client.get_object_stream(bucket.object()).await {
                    Ok(data) => {
                        Ok(HttpResponse::Ok()
                            .streaming(data.bytes.map(|res| {
                                res.map_err(actix_web::error::ErrorInternalServerError)
                            })))
                    }
                    Err(e) => {
                        error!("Failed to download file from bucket {}", e);
                        Ok(HttpResponse::InternalServerError().finish())
                    }
                }
            } else {
                error!(
                    "Bucket configuration found but no client available for {}",
                    path
                );
                Ok(HttpResponse::InternalServerError().finish())
            }
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}
