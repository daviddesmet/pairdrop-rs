//! HTTP surface, ported from `server/server.js`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::ip;
use crate::ws::{self, Hub};

pub fn router(conf: Arc<Config>, hub: Arc<Hub>) -> Router {
    // Express serves `public/` first, answers `/config` (and `/ip` in debug mode), and 301s
    // everything else to `/`.
    let assets = ServeDir::new(&conf.public_dir).append_index_html_on_directories(true);

    // The client builds its endpoint as `location.host + location.pathname + "server"`, and
    // upstream's `ws` server answers an upgrade on any path at all.
    let signaling = Router::new()
        .route("/server", axum::routing::any(ws::handler))
        .route("/server/{*rest}", axum::routing::any(ws::handler))
        .with_state(hub);

    let mut router = Router::new().route("/config", get(config));

    if conf.debug_mode {
        router = router.route("/ip", get(client_ip));
    }

    router
        .merge(signaling)
        .fallback_service(assets.fallback(axum::routing::any(redirect_to_root).with_state(conf.clone())))
        .with_state(conf)
}

async fn config(State(conf): State<Arc<Config>>) -> Json<serde_json::Value> {
    Json(conf.client_config.clone())
}

async fn client_ip(
    State(conf): State<Arc<Config>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> String {
    ip::resolve(&headers, Some(addr.ip()), &conf)
}

async fn redirect_to_root() -> Response {
    (StatusCode::MOVED_PERMANENTLY, Redirect::permanent("/")).into_response()
}
