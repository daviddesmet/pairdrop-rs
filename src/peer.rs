//! Peer identity, ported from `server/peer.js`.

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

pub struct Peer {
    pub id: Uuid,
    pub ip: String,
    pub rtc_supported: bool,
    pub name: PeerName,
    pub tx: UnboundedSender<String>,
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
