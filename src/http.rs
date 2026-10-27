//! HTTP surface, ported from upstream's `server/server.js`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::config::Config;
use crate::ip;
use crate::rate_limit::{self, RateLimiter};
use crate::sw::ServiceWorker;
use crate::ws::{self, Hub};

/// Always revalidate. The client is a PWA whose service worker fills its cache with ordinary
/// fetches, so a heuristically-cached asset would be baked into that cache for as long as the
/// cache lives. `no-cache` still allows 304s; it only forbids using a copy without asking.
const REVALIDATE: HeaderValue = HeaderValue::from_static("no-cache");

pub fn router(
    conf: Arc<Config>,
    hub: Arc<Hub>,
    service_worker: Option<Arc<ServiceWorker>>,
) -> Router {
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

    if let Some(service_worker) = service_worker {
        router = router.route(
            "/service-worker.js",
            get(move || {
                let service_worker = service_worker.clone();
                async move {
                    (
                        [
                            (
                                CONTENT_TYPE,
                                HeaderValue::from_static("application/javascript"),
                            ),
                            (CACHE_CONTROL, REVALIDATE),
                        ],
                        service_worker.body.clone(),
                    )
                }
            }),
        );
    }

    if conf.debug_mode {
        router = router.route("/ip", get(client_ip));
    }

    let mut router = router
        .merge(signaling)
        .fallback_service(
            assets.fallback(axum::routing::any(redirect_to_root).with_state(conf.clone())),
        )
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            REVALIDATE,
        ));

    if conf.rate_limit.is_some() {
        let limiter = RateLimiter::new(conf.clone());
        router = router.layer(axum::middleware::from_fn_with_state(
            limiter,
            rate_limit::layer,
        ));
    }

    router.with_state(conf)
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
