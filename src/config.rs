use std::path::PathBuf;

use serde::Serialize;

/// Mirrors `conf` in upstream's `server/index.js`.
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub localhost_only: bool,
    pub debug_mode: bool,
    pub ws_fallback: bool,
    pub rate_limit: Option<u32>,
    pub ipv6_localize: Option<usize>,
    pub signaling_server: SignalingServer,
    pub rtc_config: serde_json::Value,
    pub buttons: Buttons,
    pub public_dir: PathBuf,
    /// Pre-rendered body of `GET /config`, built once at startup.
    pub client_config: serde_json::Value,
}

/// `false` when this instance signals for itself, otherwise a host without a protocol prefix.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SignalingServer {
    Disabled(bool),
    Url(String),
}

impl SignalingServer {
    pub fn is_enabled(&self) -> bool {
        matches!(self, SignalingServer::Url(_))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Buttons {
    pub donation_button: Button,
    pub twitter_button: Button,
    pub mastodon_button: Button,
    pub bluesky_button: Button,
    pub custom_button: Button,
    pub privacypolicy_button: Button,
}

/// Unset fields are omitted entirely, the way Express drops `undefined` on serialization.
#[derive(Debug, Clone, Serialize)]
pub struct Button {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

const DEFAULT_RTC_CONFIG: &str = r#"{
    "sdpSemantics": "unified-plan",
    "iceServers": [{ "urls": "stun:stun.l.google.com:19302" }]
}"#;

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let args: Vec<String> = std::env::args().collect();
        let has_arg = |name: &str| args.iter().any(|a| a == name);

        let port = match env("PORT") {
            Some(p) => p
                .parse()
                .map_err(|_| format!("PORT must be a port number, got {p:?}"))?,
            None => 3000,
        };

        let rtc_config = match env("RTC_CONFIG").filter(|v| v != "false") {
            Some(path) => {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read RTC_CONFIG at {path}: {e}"))?;
                serde_json::from_str(&raw)
                    .map_err(|e| format!("RTC_CONFIG at {path} is not valid JSON: {e}"))?
            }
            None => serde_json::from_str(DEFAULT_RTC_CONFIG).expect("default rtc config is valid"),
        };

        let signaling_server = match env("SIGNALING_SERVER").filter(|v| v != "false") {
            Some(url) => {
                if url.contains("://") {
                    return Err(
                        "SIGNALING_SERVER must be a valid url without the protocol prefix.\n\
                         Examples of valid values: `pairdrop.net`, `pairdrop.example.com:3000`"
                            .into(),
                    );
                }
                SignalingServer::Url(if url.ends_with('/') {
                    url
                } else {
                    format!("{url}/")
                })
            }
            None => SignalingServer::Disabled(false),
        };

        let ipv6_localize = match env("IPV6_LOCALIZE").and_then(|v| v.parse::<usize>().ok()) {
            Some(0) | None => None,
            Some(n) if n < 8 => Some(n),
            Some(_) => return Err("IPV6_LOCALIZE must be an integer between 1 and 7".into()),
        };

        // `--rate-limit` / `RATE_LIMIT=true` mean "5 proxy hops"; a bare number sets the hop count.
        let rate_limit = if has_arg("--rate-limit") || env("RATE_LIMIT").as_deref() == Some("true")
        {
            Some(5)
        } else {
            env("RATE_LIMIT").and_then(|v| v.parse().ok())
        };

        let ws_fallback =
            has_arg("--include-ws-fallback") || env("WS_FALLBACK").as_deref() == Some("true");

        let config = Config {
            port,
            localhost_only: has_arg("--localhost-only"),
            debug_mode: env("DEBUG_MODE").as_deref() == Some("true"),
            ws_fallback,
            rate_limit,
            ipv6_localize,
            signaling_server,
            rtc_config,
            buttons: Buttons {
                donation_button: button("DONATION"),
                twitter_button: button("TWITTER"),
                mastodon_button: button("MASTODON"),
                bluesky_button: button("BLUESKY"),
                custom_button: button("CUSTOM"),
                privacypolicy_button: button("PRIVACYPOLICY"),
            },
            public_dir: env("PUBLIC_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| "public".into()),
            client_config: serde_json::Value::Null,
        };

        if config.signaling_server.is_enabled()
            && (config.ws_fallback || config.ipv6_localize.is_some() || env("RTC_CONFIG").is_some())
        {
            return Err(
                "SIGNALING_SERVER cannot be used alongside WS_FALLBACK, RTC_CONFIG or \
                        IPV6_LOCALIZE as these are specified by the signaling server."
                    .into(),
            );
        }

        let mut config = config;
        config.client_config = serde_json::json!({
            "signalingServer": config.signaling_server,
            "buttons": config.buttons,
        });

        Ok(config)
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn button(prefix: &str) -> Button {
    Button {
        active: env(&format!("{prefix}_BUTTON_ACTIVE")),
        link: env(&format!("{prefix}_BUTTON_LINK")),
        title: env(&format!("{prefix}_BUTTON_TITLE")),
    }
}
