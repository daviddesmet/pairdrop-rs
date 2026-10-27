//! Signaling WebSocket, ported from `server/ws-server.js`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha512};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;
use crate::ip;
use crate::peer::{Peer, PeerName};

/// Upstream pings every second and drops a peer that has not ponged for five.
const PING_INTERVAL: Duration = Duration::from_secs(1);
const PING_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Hub {
    pub conf: Arc<Config>,
    /// Salts the peer-id hashes. Regenerated per process, as upstream's `hasher` password is.
    secret: [u8; 64],
    peers: Mutex<HashMap<Uuid, Peer>>,
}

impl Hub {
    pub fn new(conf: Arc<Config>) -> Self {
        let mut secret = [0u8; 64];
        for (i, chunk) in secret.chunks_mut(16).enumerate() {
            chunk.copy_from_slice(&Uuid::new_v4().into_bytes());
            let _ = i;
        }
        Hub { conf, secret, peers: Mutex::new(HashMap::new()) }
    }

    /// Lets a reconnecting client keep its peer id without letting anyone claim an arbitrary one.
    fn peer_id_hash(&self, peer_id: &Uuid) -> String {
        let mut hasher = Sha512::new();
        hasher.update(self.secret);
        hasher.update(peer_id.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn send_to(&self, peer_id: &Uuid, message: &serde_json::Value) {
        if let Some(peer) = self.peers.lock().unwrap().get(peer_id) {
            peer.send(message);
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    peer_id: Option<String>,
    peer_id_hash: Option<String>,
    webrtc_supported: Option<String>,
}

pub async fn handler(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let ip = ip::resolve(&headers, Some(addr.ip()), &hub.conf);
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok()).map(str::to_owned);

    // Reuse the client's peer id only when it can prove the server issued it.
    let id = query
        .peer_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .filter(|id| query.peer_id_hash.as_deref() == Some(hub.peer_id_hash(id).as_str()))
        .unwrap_or_else(Uuid::new_v4);

    let rtc_supported = query.webrtc_supported.as_deref() == Some("true");

    upgrade.on_upgrade(move |socket| {
        connection(hub, socket, id, ip, rtc_supported, user_agent)
    })
}

async fn connection(
    hub: Arc<Hub>,
    socket: WebSocket,
    id: Uuid,
    ip: String,
    rtc_supported: bool,
    user_agent: Option<String>,
) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let name = PeerName::new(&id, user_agent.as_deref());
    tracing::debug!(%id, %ip, display_name = %name.display_name, "peer connected");

    hub.peers
        .lock()
        .unwrap()
        .insert(id, Peer { id, ip: ip.clone(), rtc_supported, name: name.clone(), tx: tx.clone() });

    // One task owns the sink; everything else queues through `tx`.
    let writer = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    hub.send_to(
        &id,
        &json!({
            "type": "ws-config",
            "wsConfig": {
                "rtcConfig": hub.conf.rtc_config,
                "wsFallback": hub.conf.ws_fallback,
            }
        }),
    );

    hub.send_to(
        &id,
        &json!({
            "type": "display-name",
            "displayName": name.display_name,
            "deviceName": name.device_name,
            "peerId": id,
            "peerIdHash": hub.peer_id_hash(&id),
        }),
    );

    let mut ping = tokio::time::interval(PING_INTERVAL);
    let mut last_pong = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = ping.tick() => {
                if last_pong.elapsed() > PING_TIMEOUT {
                    tracing::debug!(%id, "peer unresponsive, disconnecting");
                    break;
                }
                if tx.send(json!({ "type": "ping" }).to_string()).is_err() {
                    break;
                }
            }
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                            tracing::warn!("WS: received JSON is malformed");
                            continue;
                        };
                        match value.get("type").and_then(|t| t.as_str()) {
                            Some("pong") => last_pong = tokio::time::Instant::now(),
                            Some("disconnect") => break,
                            Some(other) => tracing::trace!(%id, kind = other, "unhandled message"),
                            None => tracing::warn!("WS: message without a type"),
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    hub.peers.lock().unwrap().remove(&id);
    drop(tx);
    writer.abort();
    tracing::debug!(%id, "peer disconnected");
}
