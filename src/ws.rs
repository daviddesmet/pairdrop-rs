//! Signaling WebSocket, ported from upstream's `server/ws-server.js`.

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
use crate::random;

/// Upstream pings every second and drops a peer that has not ponged for five.
const PING_INTERVAL: Duration = Duration::from_secs(1);
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Transfer traffic that normally rides the WebRTC data channel. Relayed only under WS_FALLBACK.
const RELAYED_ON_FALLBACK: [&str; 11] = [
    "request",
    "header",
    "partition",
    "partition-received",
    "progress",
    "files-transfer-response",
    "file-transfer-complete",
    "message-transfer-complete",
    "text",
    "display-name-changed",
    "ws-chunk",
];

/// Rooms are keyed by room id alone, as upstream's `this._rooms` is: an IP for `ip` rooms, the
/// shared secret for `secret` rooms, the room id for `public-id` rooms. Membership keeps join
/// order so `peers` arrives in the same order the JS object iterated.
#[derive(Default)]
struct HubState {
    peers: HashMap<Uuid, Peer>,
    rooms: HashMap<String, Vec<Uuid>>,
    /// Pair key -> the secret it unlocks and the peer offering it.
    pair_keys: HashMap<String, PairOffer>,
}

struct PairOffer {
    room_secret: String,
    creator: Uuid,
}

pub struct Hub {
    pub conf: Arc<Config>,
    /// Salts the peer-id hashes. Regenerated per process, as upstream's `hasher` password is.
    secret: [u8; 64],
    state: Mutex<HubState>,
}

impl Hub {
    pub fn new(conf: Arc<Config>) -> Self {
        let mut secret = [0u8; 64];
        for (i, chunk) in secret.chunks_mut(16).enumerate() {
            chunk.copy_from_slice(&Uuid::new_v4().into_bytes());
            let _ = i;
        }
        Hub {
            conf,
            secret,
            state: Mutex::new(HubState::default()),
        }
    }

    /// Lets a reconnecting client keep its peer id without letting anyone claim an arbitrary one.
    fn peer_id_hash(&self, peer_id: &Uuid) -> String {
        let mut hasher = Sha512::new();
        hasher.update(self.secret);
        hasher.update(peer_id.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn send_to(&self, peer_id: &Uuid, message: &serde_json::Value) {
        if let Some(peer) = self.state.lock().unwrap().peers.get(peer_id) {
            peer.send(message);
        }
    }

    fn join_ip_room(&self, peer_id: &Uuid) {
        let mut state = self.state.lock().unwrap();
        let Some(room_id) = state.peers.get(peer_id).map(|p| p.ip.clone()) else {
            return;
        };
        state.join_room(peer_id, "ip", &room_id);
    }

    fn on_room_secrets(&self, peer_id: &Uuid, message: &serde_json::Value) {
        let Some(secrets) = message.get("roomSecrets").and_then(|s| s.as_array()) else {
            return;
        };

        let secrets: Vec<String> = secrets
            .iter()
            .filter_map(|s| s.as_str())
            .filter(|s| s.is_ascii() && (64..=256).contains(&s.chars().count()))
            .map(str::to_owned)
            .collect();

        let mut state = self.state.lock().unwrap();
        for secret in secrets {
            state.join_secret_room(peer_id, &secret);
        }
    }

    fn on_room_secrets_deleted(&self, message: &serde_json::Value) {
        let Some(secrets) = message.get("roomSecrets").and_then(|s| s.as_array()) else {
            return;
        };

        let mut state = self.state.lock().unwrap();
        for secret in secrets.iter().filter_map(|s| s.as_str()) {
            state.delete_secret_room(secret);
        }
    }

    fn on_pair_device_initiate(&self, peer_id: &Uuid) {
        let room_secret = random::room_secret();
        let mut state = self.state.lock().unwrap();

        let pair_key = state.create_pair_key(peer_id, &room_secret);

        if let Some(previous) = state.peers.get(peer_id).and_then(|p| p.pair_key.clone()) {
            if previous != pair_key {
                state.remove_pair_key(&previous);
            }
        }
        if let Some(peer) = state.peers.get_mut(peer_id) {
            peer.pair_key = Some(pair_key.clone());
            peer.send(&json!({
                "type": "pair-device-initiated",
                "roomSecret": room_secret,
                "pairKey": pair_key,
            }));
        }

        state.join_secret_room(peer_id, &room_secret);
    }

    fn on_pair_device_join(&self, peer_id: &Uuid, message: &serde_json::Value) {
        let mut state = self.state.lock().unwrap();

        let rate_limited = state
            .peers
            .get_mut(peer_id)
            .is_some_and(|peer| peer.rate_limit_reached());
        if rate_limited {
            state.send(peer_id, &json!({ "type": "join-key-rate-limit" }));
            return;
        }

        let key = message
            .get("pairKey")
            .and_then(|k| k.as_str())
            .unwrap_or_default()
            .to_string();
        let offer = state.pair_keys.get(&key);

        let Some((room_secret, creator)) = offer
            .filter(|offer| offer.creator != *peer_id)
            .map(|offer| (offer.room_secret.clone(), offer.creator))
        else {
            state.send(peer_id, &json!({ "type": "pair-device-join-key-invalid" }));
            return;
        };

        state.remove_pair_key(&key);

        state.send(
            peer_id,
            &json!({ "type": "pair-device-joined", "roomSecret": room_secret, "peerId": creator }),
        );
        state.send(
            &creator,
            &json!({ "type": "pair-device-joined", "roomSecret": room_secret, "peerId": peer_id }),
        );

        state.join_secret_room(peer_id, &room_secret);

        // A device that was itself offering a key stops offering it once it pairs.
        if let Some(own_key) = state.peers.get(peer_id).and_then(|p| p.pair_key.clone()) {
            state.remove_pair_key(&own_key);
        }
    }

    fn on_pair_device_cancel(&self, peer_id: &Uuid) {
        let mut state = self.state.lock().unwrap();
        let Some(pair_key) = state.peers.get(peer_id).and_then(|p| p.pair_key.clone()) else {
            return;
        };

        state.remove_pair_key(&pair_key);
        state.send(
            peer_id,
            &json!({ "type": "pair-device-canceled", "pairKey": pair_key }),
        );
    }

    fn on_regenerate_room_secret(&self, message: &serde_json::Value) {
        let Some(old_secret) = message.get("roomSecret").and_then(|s| s.as_str()) else {
            return;
        };
        let new_secret = random::room_secret();

        let mut state = self.state.lock().unwrap();
        let Some(members) = state.rooms.remove(old_secret) else {
            return;
        };

        let notice = json!({
            "type": "room-secret-regenerated",
            "oldRoomSecret": old_secret,
            "newRoomSecret": new_secret,
        });

        for member_id in members {
            if let Some(peer) = state.peers.get_mut(&member_id) {
                peer.send(&notice);
                peer.remove_room_secret(old_secret);
            }
        }
    }

    fn on_create_public_room(&self, peer_id: &Uuid) {
        let room_id = random::public_room_id();

        let mut state = self.state.lock().unwrap();
        state.send(
            peer_id,
            &json!({ "type": "public-room-created", "roomId": room_id }),
        );
        state.join_public_room(peer_id, &room_id);
    }

    fn on_join_public_room(&self, peer_id: &Uuid, message: &serde_json::Value) {
        let mut state = self.state.lock().unwrap();

        let rate_limited = state
            .peers
            .get_mut(peer_id)
            .is_some_and(|peer| peer.rate_limit_reached());
        if rate_limited {
            state.send(peer_id, &json!({ "type": "join-key-rate-limit" }));
            return;
        }

        let Some(room_id) = message
            .get("publicRoomId")
            .and_then(|id| id.as_str())
            .map(str::to_owned)
        else {
            return;
        };
        let create_if_invalid = message
            .get("createIfInvalid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !state.rooms.contains_key(&room_id) && !create_if_invalid {
            state.send(
                peer_id,
                &json!({ "type": "public-room-id-invalid", "publicRoomId": room_id }),
            );
            return;
        }

        state.join_public_room(peer_id, &room_id);
    }

    fn on_leave_public_room(&self, peer_id: &Uuid) {
        let mut state = self.state.lock().unwrap();
        state.leave_public_room(peer_id, true);
        state.send(peer_id, &json!({ "type": "public-room-left" }));
    }

    /// Everything a peer is part of falls away together when the socket closes.
    fn disconnect(&self, peer_id: &Uuid) {
        let mut state = self.state.lock().unwrap();

        if let Some(pair_key) = state.peers.get(peer_id).and_then(|p| p.pair_key.clone()) {
            state.remove_pair_key(&pair_key);
        }

        if let Some(ip) = state.peers.get(peer_id).map(|p| p.ip.clone()) {
            state.leave_room(peer_id, "ip", &ip, true);
        }

        for secret in state
            .peers
            .get(peer_id)
            .map(|p| p.room_secrets.clone())
            .unwrap_or_default()
        {
            state.leave_secret_room(peer_id, &secret, true);
        }

        state.leave_public_room(peer_id, true);
        state.peers.remove(peer_id);
    }

    /// `_signalAndRelay`: hand the message to `message.to` within the sender's room, stamped with
    /// the sender's identity. Only peers that share a room can reach each other.
    fn signal_and_relay(&self, sender_id: &Uuid, mut message: serde_json::Value) {
        let state = self.state.lock().unwrap();
        let Some(sender) = state.peers.get(sender_id) else {
            return;
        };

        let room_id = match message.get("roomType").and_then(|t| t.as_str()) {
            Some("ip") => sender.ip.clone(),
            _ => match message.get("roomId").and_then(|id| id.as_str()) {
                Some(id) => id.to_string(),
                None => return,
            },
        };

        let Some(recipient_id) = message
            .get("to")
            .and_then(|to| to.as_str())
            .and_then(|to| Uuid::parse_str(to).ok())
        else {
            return;
        };

        let in_room = state
            .rooms
            .get(&room_id)
            .is_some_and(|m| m.contains(&recipient_id));
        if !in_room {
            return;
        }

        let sender_info = json!({ "id": sender.id, "rtcSupported": sender.rtc_supported });
        if let Some(object) = message.as_object_mut() {
            object.remove("to");
            object.insert("sender".into(), sender_info);
        }

        if let Some(recipient) = state.peers.get(&recipient_id) {
            recipient.send(&message);
        }
    }
}

impl HubState {
    fn send(&self, peer_id: &Uuid, message: &serde_json::Value) {
        if let Some(peer) = self.peers.get(peer_id) {
            peer.send(message);
        }
    }

    fn join_secret_room(&mut self, peer_id: &Uuid, room_secret: &str) {
        self.join_room(peer_id, "secret", room_secret);
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.add_room_secret(room_secret);
        }
    }

    fn leave_secret_room(&mut self, peer_id: &Uuid, room_secret: &str, disconnect: bool) {
        self.leave_room(peer_id, "secret", room_secret, disconnect);
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.remove_room_secret(room_secret);
        }
    }

    fn delete_secret_room(&mut self, room_secret: &str) {
        let Some(members) = self.rooms.get(room_secret).cloned() else {
            return;
        };

        let notice = json!({ "type": "secret-room-deleted", "roomSecret": room_secret });
        for member_id in members {
            self.leave_secret_room(&member_id, room_secret, true);
            self.send(&member_id, &notice);
        }
    }

    /// A peer is only ever in one public room.
    fn join_public_room(&mut self, peer_id: &Uuid, room_id: &str) {
        self.leave_public_room(peer_id, false);
        self.join_room(peer_id, "public-id", room_id);
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.public_room_id = Some(room_id.to_string());
        }
    }

    fn leave_public_room(&mut self, peer_id: &Uuid, disconnect: bool) {
        let Some(room_id) = self
            .peers
            .get(peer_id)
            .and_then(|p| p.public_room_id.clone())
        else {
            return;
        };

        self.leave_room(peer_id, "public-id", &room_id, disconnect);
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.public_room_id = None;
        }
    }

    fn create_pair_key(&mut self, creator: &Uuid, room_secret: &str) -> String {
        let mut key = random::pair_key();
        while self.pair_keys.contains_key(&key) {
            key = random::pair_key();
        }

        self.pair_keys.insert(
            key.clone(),
            PairOffer {
                room_secret: room_secret.to_string(),
                creator: *creator,
            },
        );
        key
    }

    fn remove_pair_key(&mut self, pair_key: &str) {
        if let Some(offer) = self.pair_keys.remove(pair_key) {
            if let Some(creator) = self.peers.get_mut(&offer.creator) {
                creator.pair_key = None;
            }
        }
    }

    fn join_room(&mut self, peer_id: &Uuid, room_type: &str, room_id: &str) {
        // Re-joining leaves first, so other peers never see `peer-left` after `peer-joined`.
        if self.rooms.get(room_id).is_some_and(|m| m.contains(peer_id)) {
            self.leave_room(peer_id, room_type, room_id, false);
        }

        // The room has to exist before notifying, so the first peer in still gets `peers: []`.
        self.rooms.entry(room_id.to_string()).or_default();
        self.notify_peers(peer_id, room_type, room_id);
        self.rooms
            .get_mut(room_id)
            .expect("room was just created")
            .push(*peer_id);
    }

    fn leave_room(&mut self, peer_id: &Uuid, room_type: &str, room_id: &str, disconnect: bool) {
        let Some(members) = self.rooms.get_mut(room_id) else {
            return;
        };
        let Some(index) = members.iter().position(|id| id == peer_id) else {
            return;
        };
        members.remove(index);

        if members.is_empty() {
            self.rooms.remove(room_id);
            return;
        }

        let message = json!({
            "type": "peer-left",
            "peerId": peer_id,
            "roomType": room_type,
            "roomId": room_id,
            "disconnect": disconnect,
        });

        for other_id in self.rooms[room_id].clone() {
            if let Some(other) = self.peers.get(&other_id) {
                other.send(&message);
            }
        }
    }

    /// Tells the room about the newcomer, then tells the newcomer about the room.
    fn notify_peers(&self, peer_id: &Uuid, room_type: &str, room_id: &str) {
        let Some(members) = self.rooms.get(room_id) else {
            return;
        };
        let Some(peer) = self.peers.get(peer_id) else {
            return;
        };

        let joined = json!({
            "type": "peer-joined",
            "peer": peer.info(),
            "roomType": room_type,
            "roomId": room_id,
        });

        let mut others = Vec::new();
        for other_id in members.iter().filter(|id| *id != peer_id) {
            if let Some(other) = self.peers.get(other_id) {
                other.send(&joined);
                others.push(other.info());
            }
        }

        peer.send(&json!({
            "type": "peers",
            "peers": others,
            "roomType": room_type,
            "roomId": room_id,
        }));
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
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // Reuse the client's peer id only when it can prove the server issued it.
    let id = query
        .peer_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .filter(|id| query.peer_id_hash.as_deref() == Some(hub.peer_id_hash(id).as_str()))
        .unwrap_or_else(Uuid::new_v4);

    let rtc_supported = query.webrtc_supported.as_deref() == Some("true");

    upgrade.on_upgrade(move |socket| connection(hub, socket, id, ip, rtc_supported, user_agent))
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

    hub.state.lock().unwrap().peers.insert(
        id,
        Peer::new(id, ip.clone(), rtc_supported, name.clone(), tx.clone()),
    );

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
                            Some("join-ip-room") => hub.join_ip_room(&id),
                            Some("room-secrets") => hub.on_room_secrets(&id, &value),
                            Some("room-secrets-deleted") => hub.on_room_secrets_deleted(&value),
                            Some("pair-device-initiate") => hub.on_pair_device_initiate(&id),
                            Some("pair-device-join") => hub.on_pair_device_join(&id, &value),
                            Some("pair-device-cancel") => hub.on_pair_device_cancel(&id),
                            Some("regenerate-room-secret") => hub.on_regenerate_room_secret(&value),
                            Some("create-public-room") => hub.on_create_public_room(&id),
                            Some("join-public-room") => hub.on_join_public_room(&id, &value),
                            Some("leave-public-room") => hub.on_leave_public_room(&id),
                            Some("signal") => hub.signal_and_relay(&id, value),
                            // Everything else is peer-to-peer payload that only travels through
                            // the server when the WebSocket fallback is switched on.
                            Some(kind) if RELAYED_ON_FALLBACK.contains(&kind) => {
                                if hub.conf.ws_fallback {
                                    hub.signal_and_relay(&id, value);
                                } else {
                                    tracing::debug!(kind, "websocket fallback is not activated on this instance");
                                }
                            }
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

    hub.disconnect(&id);
    drop(tx);
    writer.abort();
    tracing::debug!(%id, "peer disconnected");
}
