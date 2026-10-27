//! Peer identity, ported from upstream's `server/peer.js`.

use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::names;

/// What the client receives for every peer it can see.
#[derive(Debug, Clone, Serialize)]
pub struct PeerName {
    pub model: Option<String>,
    pub os: Option<String>,
    pub browser: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(rename = "deviceName")]
    pub device_name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

impl PeerName {
    pub fn new(peer_id: &Uuid, user_agent: Option<&str>) -> Self {
        PeerName {
            model: None,
            os: None,
            browser: None,
            kind: None,
            device_name: names::device_name(user_agent),
            display_name: names::display_name(&peer_id.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerInfo {
    pub id: Uuid,
    pub name: PeerName,
    #[serde(rename = "rtcSupported")]
    pub rtc_supported: bool,
}

/// `rateLimitReached` in peer.js: at most 10 key attempts in any 10 second window.
const RATE_LIMIT_ATTEMPTS: usize = 10;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(10);

pub struct Peer {
    pub id: Uuid,
    pub ip: String,
    pub rtc_supported: bool,
    pub name: PeerName,
    pub tx: UnboundedSender<String>,
    /// Secrets of every paired room this peer is currently in.
    pub room_secrets: Vec<String>,
    /// Set while this peer is offering a pair key for someone else to enter.
    pub pair_key: Option<String>,
    pub public_room_id: Option<String>,
    recent_attempts: Vec<Instant>,
}

impl Peer {
    pub fn new(
        id: Uuid,
        ip: String,
        rtc_supported: bool,
        name: PeerName,
        tx: UnboundedSender<String>,
    ) -> Self {
        Peer {
            id,
            ip,
            rtc_supported,
            name,
            tx,
            room_secrets: Vec::new(),
            pair_key: None,
            public_room_id: None,
            recent_attempts: Vec::new(),
        }
    }

    pub fn rate_limit_reached(&mut self) -> bool {
        let now = Instant::now();
        self.recent_attempts.retain(|at| now.duration_since(*at) < RATE_LIMIT_WINDOW);

        if self.recent_attempts.len() >= RATE_LIMIT_ATTEMPTS {
            return true;
        }

        self.recent_attempts.push(now);
        false
    }

    pub fn add_room_secret(&mut self, room_secret: &str) {
        if !self.room_secrets.iter().any(|s| s == room_secret) {
            self.room_secrets.push(room_secret.to_string());
        }
    }

    pub fn remove_room_secret(&mut self, room_secret: &str) {
        self.room_secrets.retain(|s| s != room_secret);
    }
}

impl Peer {
    pub fn info(&self) -> PeerInfo {
        PeerInfo { id: self.id, name: self.name.clone(), rtc_supported: self.rtc_supported }
    }

    pub fn send(&self, message: &serde_json::Value) {
        // A full or closed channel just means the peer is already gone.
        let _ = self.tx.send(message.to_string());
    }
}
