/// Configuration loaded from `~/.clerk/config.toml`.
///
/// Search order:
/// 1. `$CLERK_CONFIG` env var
/// 2. `~/.clerk/config.toml`
///
/// After loading, `CLERK_*` env vars override individual fields.
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root data directory for all clerk files.
    pub data_dir: PathBuf,
    /// Maximum content length per file in characters.
    pub max_content_length: usize,
    /// HTTP transport bind address.
    pub http_bind: Option<String>,
    /// Log level filter string.
    pub log_level: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            max_content_length: 10_000,
            http_bind: None,
            log_level: None,
        }
    }
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".clerk"))
        .unwrap_or_else(|| PathBuf::from(".clerk"))
}

impl Config {
    /// Load config from well-known paths, falling back to defaults.
    ///
    /// After loading from file (or defaults), environment variables with the
    /// `CLERK_` prefix override individual fields.
    pub fn load() -> Self {
        let candidates: Vec<PathBuf> = [
            std::env::var("CLERK_CONFIG").ok().map(PathBuf::from),
            dirs::home_dir().map(|d| d.join(".clerk").join("config.toml")),
        ]
        .into_iter()
        .flatten()
        .collect();

        let mut config = None;
        for path in &candidates {
            if let Some(c) = Self::try_load(path) {
                tracing::info!(?path, "loaded configuration");
                config = Some(c);
                break;
            }
        }

        let mut config = config.unwrap_or_else(|| {
            tracing::debug!("no config file found, using defaults");
            Self::default()
        });
        config.apply_env_overrides();
        config
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("CLERK_DATA_DIR") {
            tracing::debug!(CLERK_DATA_DIR = %val, "env override");
            self.data_dir = PathBuf::from(val);
        }
        if let Ok(val) = std::env::var("CLERK_MAX_CONTENT_LENGTH") {
            if let Ok(v) = val.parse::<usize>() {
                tracing::debug!(CLERK_MAX_CONTENT_LENGTH = %val, "env override");
                self.max_content_length = v;
            } else {
                tracing::warn!(CLERK_MAX_CONTENT_LENGTH = %val, "ignoring non-numeric value");
            }
        }
        if let Ok(val) = std::env::var("CLERK_HTTP_BIND") {
            tracing::debug!(CLERK_HTTP_BIND = %val, "env override");
            self.http_bind = Some(val);
        }
        if let Ok(val) = std::env::var("CLERK_LOG_LEVEL") {
            tracing::debug!(CLERK_LOG_LEVEL = %val, "env override");
            self.log_level = Some(val);
        }
    }

    fn try_load(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        match toml::from_str::<Config>(&content) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!(?path, %e, "invalid config file, skipping");
                None
            }
        }
    }
}
