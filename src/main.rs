mod config;
mod http;
mod ip;
mod names;
mod peer;
mod random;
mod ws;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use config::Config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pairdrop_rs=info,tower_http=warn".into()),
        )
        .init();

    let conf = match Config::from_env() {
        Ok(conf) => Arc::new(conf),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    if conf.debug_mode {
        tracing::warn!("DEBUG_MODE is active. To protect privacy, do not use in production.");
        tracing::debug!(?conf, "configuration");
    }

    if let Some(segments) = conf.ipv6_localize {
        tracing::info!(segments, "IPv6 client IPs will be localized");
    }

    let host = if conf.localhost_only { Ipv4Addr::LOCALHOST } else { Ipv4Addr::UNSPECIFIED };
    let addr = SocketAddr::from((host, conf.port));

    let hub = Arc::new(ws::Hub::new(conf.clone()));
    let app = http::router(conf.clone(), hub);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("could not bind {addr}: {err}");
            std::process::exit(1);
        }
    };

    tracing::info!(%addr, public_dir = ?conf.public_dir, "PairDrop is running");

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server failed");
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl-c handler") };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("SIGINT received, exiting"),
        _ = terminate => tracing::info!("SIGTERM received, exiting"),
    }
}
