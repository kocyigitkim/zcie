//! HTTP/JSON gateway. It changes HTTP requests into `zcie-api` calls. It runs inside `zcied`.

pub mod auth;
pub mod error;
pub mod routes;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use zcie_api::Engine;

pub use routes::router;

/// Serves the API on `addr` until `shutdown` completes.
pub async fn serve(engine: Arc<Engine>, addr: SocketAddr, max_payload_bytes: usize, shutdown: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "HTTP gateway listening");
    axum::serve(listener, router(engine, max_payload_bytes)).with_graceful_shutdown(shutdown).await
}
