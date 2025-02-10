pub mod asyncrs;
use asyncrs::worker::{http_camera_feed, mjpeg_stream, tcp_async, TransmissionType};
use async_std::{sync::RwLock, task};
use async_std::sync::Arc;

#[async_std::main]
async fn main() -> std::io::Result<()> {

    let pipe: TransmissionType = Arc::new(RwLock::new(None));
    let copy_pipe = Arc::clone(&pipe);

    let handle = task::spawn( async move { tcp_async("0.0.0.0", "3151", Arc::clone(&pipe)).await } );
    let handle2 = task::spawn( async move { mjpeg_stream("0.0.0.0", "5000", Arc::clone(&copy_pipe)).await } );
    http_camera_feed("0.0.0.0", "8080").await?;
    handle.await?;
    handle2.await?;
    return Ok(());
}
