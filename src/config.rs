use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Sampling period in milliseconds.
    pub sample_interval_ms: u64,

    /// Number of history points retained per metric (ring buffer capacity).
    pub history_capacity: usize,

    /// UI redraw period in milliseconds.
    pub ui_tick_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sample_interval_ms: 1000,
            history_capacity: 600,
            ui_tick_ms: 100,
        }
    }
}

impl Config {
    pub fn sample_interval(&self) -> Duration {
        Duration::from_millis(self.sample_interval_ms)
    }

    pub fn ui_tick(&self) -> Duration {
        Duration::from_millis(self.ui_tick_ms)
    }

    /// Load config from `override_path` if provided, otherwise from the XDG path.
    /// Missing file returns defaults; malformed file is an error.
    pub fn load(override_path: Option<&Path>) -> Result<Self> {
        let path = match override_path {
            Some(p) => Some(p.to_path_buf()),
            None => default_config_path(),
        };

        let Some(path) = path else {
            tracing::debug!("no config path determinable, using defaults");
            return Ok(Self::default());
        };

        if !path.exists() {
            tracing::debug!(?path, "config file not found, using defaults");
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(cfg)
    }
}

fn default_config_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "resource-monitor")?;
    Some(dirs.config_dir().join("config.toml"))
}
