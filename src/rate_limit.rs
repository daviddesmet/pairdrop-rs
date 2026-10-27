//! Per-IP request limiting, replacing `express-rate-limit` in upstream's `server/server.js`.
//!
//! Like upstream, the key is the client IP as the proxy reports it, so an instance exposed
//! without a trusted proxy in front can be made to see whatever `X-Forwarded-For` a caller sends.
//! Bind the container to loopback and let the proxy be the only way in.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::Config;
use crate::ip;

const WINDOW: Duration = Duration::from_secs(5 * 60);
const MAX_REQUESTS: u32 = 1000;
const MESSAGE: &str = "Too many requests from this IP Address, please try again after 5 minutes.";

#[derive(Clone)]
pub struct RateLimiter {
    conf: Arc<Config>,
    windows: Arc<Mutex<HashMap<String, Window>>>,
}

struct Window {
    started: Instant,
    count: u32,
}

impl RateLimiter {
    pub fn new(conf: Arc<Config>) -> Self {
        RateLimiter { conf, windows: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Returns how many requests remain, or `None` once the window is exhausted.
    fn take(&self, client_ip: String) -> Option<u32> {
        let now = Instant::now();
        let mut windows = self.windows.lock().unwrap();

        windows.retain(|_, window| now.duration_since(window.started) < WINDOW);

        let window = windows.entry(client_ip).or_insert(Window { started: now, count: 0 });

        if window.count >= MAX_REQUESTS {
            return None;
        }

        window.count += 1;
        Some(MAX_REQUESTS - window.count)
    }
}

pub async fn layer(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let client_ip = ip::resolve(request.headers(), Some(addr.ip()), &limiter.conf);

    let Some(remaining) = limiter.take(client_ip) else {
        return (StatusCode::TOO_MANY_REQUESTS, MESSAGE).into_response();
    };

    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("ratelimit-limit", HeaderValue::from(MAX_REQUESTS));
    headers.insert("ratelimit-remaining", HeaderValue::from(remaining));
    headers.insert("ratelimit-reset", HeaderValue::from(WINDOW.as_secs()));
    response
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{RateLimiter, MAX_REQUESTS};
    use crate::config::Config;

    #[test]
    fn exhausts_one_ip_without_touching_another() {
        std::env::set_var("PORT", "3000");
        let limiter = RateLimiter::new(Arc::new(Config::from_env().unwrap()));

        for _ in 0..MAX_REQUESTS {
            assert!(limiter.take("198.51.100.4".into()).is_some());
        }

        assert!(limiter.take("198.51.100.4".into()).is_none());
        assert!(limiter.take("198.51.100.5".into()).is_some());
    }
}
