use crate::data::DownloadData;
use actix_web::{HttpResponse, web};
use tokio_stream::StreamExt;

pub fn get_config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("{tail:.*}").route(web::get().to(download)));
}

async fn download(
    path: web::Path<String>,
    download_data: web::Data<DownloadData>,
) -> core::result::Result<HttpResponse, actix_web::Error> {
    // Only allow specific paths
    info!("Received request for path: {}", path);
    match download_data.get_entry(&path) {
        Some(bucket) => {
            info!("Valid path request!");

            if let Some(bucket_client) = download_data.buckets().get(path.as_str()) {
                match bucket_client.get_object_stream(bucket.file()).await {
                    Ok(data) => {
                        Ok(HttpResponse::Ok().streaming(data.bytes.map(Ok::<_, actix_web::Error>)))
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
