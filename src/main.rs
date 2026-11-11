use std::{
    net::{Ipv6Addr, SocketAddr},
    sync::atomic::Ordering,
};

use anyhow::{Context, Result};
use notes_stats::{AppState, config::Config, notes_api::NotesApi, routes};
use tokio::{net::TcpListener, signal};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = Config::from_env().context("reading the configuration")?;

    let notes_api = NotesApi::new(
        &config.notes_api_url,
        config.notes_api_token_file.clone(),
        config.upstream_timeout,
    )?;

    let delay = config.shutdown_delay;
    // `[::]`: IPv6 and, with the Linux default bindv6only=0, IPv4 too. The cluster is IPv6-first:
    // an IPv4-only listener refuses the kubelet probes on the pod's IPv6 address.
    let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, config.port));
    let state = AppState::new(config, notes_api);
    let ready = state.ready.clone();
    let app = routes::router(state);

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("shutting down: not ready, draining");
            ready.store(false, Ordering::Relaxed);
            tokio::time::sleep(delay).await;
        })
        .await
        .context("serving")?;
    tracing::info!("stopped");
    Ok(())
}

/// JSON logs on stdout; the level from `RUST_LOG`, else `LOG_LEVEL`, else info.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            EnvFilter::try_new(std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()))
        })
        .unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(false)
        .init();
}

/// Resolves on SIGTERM (the kubelet) or Ctrl-C.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
