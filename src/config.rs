use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub irc: IrcConfig,
    pub meshtastic: MeshtasticConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrcConfig {
    pub server: String,
    pub port: u16,
    pub channel: String,
    pub nickname: String,
    pub username: Option<String>,
    pub realname: Option<String>,
    pub password: Option<String>,
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshtasticConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_port: Option<PathBuf>,
    pub channel: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            irc: IrcConfig {
                server: "irc.libera.chat".to_string(),
                port: 6697,
                channel: "#meshtastic".to_string(),
                nickname: "meshtastic-bridge".to_string(),
                username: None,
                realname: None,
                password: None,
                use_tls: true,
            },
            meshtastic: MeshtasticConfig {
                serial_port: None, // Will be auto-detected
                channel: 0,
            },
        }
    }
}