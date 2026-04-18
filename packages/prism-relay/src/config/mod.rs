//! Relay configuration — multi-source config with mode-specific defaults.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayMode {
    Server,
    P2p,
    #[default]
    Dev,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bootstrap_peers: Vec<BootstrapPeer>,
    #[serde(default)]
    pub public_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPeer {
    pub relay_did: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String {
    "info".into()
}
fn default_log_format() -> String {
    "text".into()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub listed: bool,
}

impl Default for DirectoryConfig {
    fn default() -> Self {
        Self {
            name: "Prism Relay".into(),
            description: String::new(),
            listed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfig {
    #[serde(default)]
    pub mode: RelayMode,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub did_method: String,
    #[serde(default)]
    pub did_web_domain: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default = "default_hashcash_bits")]
    pub hashcash_bits: u32,
    #[serde(default)]
    pub cors_origins: Vec<String>,
    #[serde(default)]
    pub federation: FederationConfig,
    #[serde(default = "default_ttl")]
    pub default_ttl_ms: u64,
    #[serde(default = "default_max_envelope")]
    pub max_envelope_size_bytes: usize,
    #[serde(default = "default_eviction_interval")]
    pub eviction_interval_ms: u64,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub directory: DirectoryConfig,
}

fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    1420
}
fn default_data_dir() -> PathBuf {
    PathBuf::from("~/.prism/relay")
}
fn default_hashcash_bits() -> u32 {
    16
}
fn default_ttl() -> u64 {
    7 * 24 * 60 * 60 * 1000
}
fn default_max_envelope() -> usize {
    1_048_576
}
fn default_eviction_interval() -> u64 {
    60_000
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            mode: RelayMode::Dev,
            host: default_host(),
            port: default_port(),
            data_dir: default_data_dir(),
            did_method: "key".into(),
            did_web_domain: None,
            modules: Vec::new(),
            hashcash_bits: default_hashcash_bits(),
            cors_origins: Vec::new(),
            federation: FederationConfig::default(),
            default_ttl_ms: default_ttl(),
            max_envelope_size_bytes: default_max_envelope(),
            eviction_interval_ms: default_eviction_interval(),
            logging: LoggingConfig::default(),
            directory: DirectoryConfig::default(),
        }
    }
}

impl RelayConfig {
    pub fn for_mode(mode: RelayMode) -> Self {
        let mut config = Self {
            mode,
            ..Self::default()
        };
        match mode {
            RelayMode::Server => {
                config.host = "0.0.0.0".into();
                config.hashcash_bits = 16;
                config.logging.format = "json".into();
            }
            RelayMode::P2p => {
                config.hashcash_bits = 12;
                config.federation.enabled = true;
            }
            RelayMode::Dev => {
                config.hashcash_bits = 4;
                config.cors_origins = vec!["*".into()];
                config.logging.level = "debug".into();
            }
        }
        config
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn from_env(mut self) -> Self {
        if let Ok(mode) = std::env::var("PRISM_RELAY_MODE") {
            self.mode = match mode.as_str() {
                "server" => RelayMode::Server,
                "p2p" => RelayMode::P2p,
                _ => RelayMode::Dev,
            };
        }
        if let Ok(host) = std::env::var("PRISM_RELAY_HOST") {
            self.host = host;
        }
        if let Ok(port) = std::env::var("PRISM_RELAY_PORT") {
            if let Ok(p) = port.parse() {
                self.port = p;
            }
        }
        if let Ok(dir) = std::env::var("PRISM_RELAY_DATA_DIR") {
            self.data_dir = PathBuf::from(dir);
        }
        if let Ok(url) = std::env::var("PRISM_RELAY_PUBLIC_URL") {
            self.federation.public_url = Some(url);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_defaults() {
        let dev = RelayConfig::for_mode(RelayMode::Dev);
        assert_eq!(dev.hashcash_bits, 4);
        assert_eq!(dev.cors_origins, vec!["*"]);

        let server = RelayConfig::for_mode(RelayMode::Server);
        assert_eq!(server.hashcash_bits, 16);
        assert_eq!(server.host, "0.0.0.0");

        let p2p = RelayConfig::for_mode(RelayMode::P2p);
        assert!(p2p.federation.enabled);
    }

    #[test]
    fn bind_addr() {
        let config = RelayConfig::default();
        assert_eq!(config.bind_addr(), "127.0.0.1:1420");
    }
}
