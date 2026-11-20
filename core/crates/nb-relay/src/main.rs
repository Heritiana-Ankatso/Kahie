//! `kahie-relay`: run the sync relay.
//!
//! Environment: `KAHIE_RELAY_ADDR` (default `127.0.0.1:8787`), `KAHIE_RELAY_DATA`
//! (default `./relay-data`), `KAHIE_RELAY_TOKEN` (shared secret; required unless the
//! address is a loopback address). Put it behind a TLS reverse proxy for internet use.

use nb_relay::{router, Config};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let addr: SocketAddr = std::env::var("KAHIE_RELAY_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()
        .expect("KAHIE_RELAY_ADDR must be host:port");
    let token = std::env::var("KAHIE_RELAY_TOKEN").ok().filter(|t| !t.is_empty());
    if token.is_none() && !addr.ip().is_loopback() {
        eprintln!("refusing to listen on {addr} without KAHIE_RELAY_TOKEN");
        std::process::exit(2);
    }
    let cfg = Config {
        data_dir: std::env::var("KAHIE_RELAY_DATA")
            .unwrap_or_else(|_| "relay-data".into())
            .into(),
        token,
    };
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    eprintln!("kahie-relay listening on {addr}, data in {}", cfg.data_dir.display());
    axum::serve(listener, router(cfg))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve");
}
