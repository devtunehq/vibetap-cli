//! Configuration management for VibeTap
//!
//! Handles loading and saving configuration from:
//! - Global config: ~/.config/vibetap/config.toml
//! - Project config: .aitest/config.json

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Read(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(String),

    #[error("No API key configured. Run 'vibetap auth login' first.")]
    NoApiKey,
}

/// Global configuration (stored in ~/.config/vibetap/)
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

/// Project-level configuration (stored in .aitest/)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub version: String,
    pub project_type: String,
    pub test_runner: String,
    pub watch_mode: WatchModeConfig,
    pub generation: GenerationConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchModeConfig {
    pub enabled: bool,
    pub debounce_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    pub max_suggestions: u32,
    pub include_security: bool,
    pub include_negative_paths: bool,
}

/// Combined configuration from global and project sources
pub struct Config {
    pub global: GlobalConfig,
    pub project: Option<ProjectConfig>,
}

impl Config {
    /// Load configuration from all sources
    pub fn load() -> Result<Self, ConfigError> {
        let global = Self::load_global()?;
        let project = Self::load_project().ok();

        Ok(Self { global, project })
    }

    /// Get the API URL (with default fallback)
    pub fn api_url(&self) -> &str {
        self.global
            .api_url
            .as_deref()
            .unwrap_or("https://vibetap.dev")
    }

    /// Get the API key (or error if not set)
    pub fn api_key(&self) -> Result<&str, ConfigError> {
        self.global
            .api_key
            .as_deref()
            .ok_or(ConfigError::NoApiKey)
    }

    /// Get the global config directory
    pub fn global_config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("vibetap")
    }

    /// Get the global config file path
    pub fn global_config_path() -> PathBuf {
        Self::global_config_dir().join("config.toml")
    }

    /// Load global configuration
    fn load_global() -> Result<GlobalConfig, ConfigError> {
        let path = Self::global_config_path();

        if !path.exists() {
            return Ok(GlobalConfig::default());
        }

        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Load project configuration
    fn load_project() -> Result<ProjectConfig, ConfigError> {
        let path = Path::new(".aitest/config.json");

        if !path.exists() {
            return Err(ConfigError::Read(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Project config not found",
            )));
        }

        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Save global configuration
    pub fn save_global(config: &GlobalConfig) -> Result<(), ConfigError> {
        let dir = Self::global_config_dir();
        std::fs::create_dir_all(&dir)?;

        let path = Self::global_config_path();
        let content = toml::to_string_pretty(config).map_err(|e| ConfigError::Parse(e.to_string()))?;
        std::fs::write(path, content)?;

        Ok(())
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            project_type: "node".to_string(),
            test_runner: "vitest".to_string(),
            watch_mode: WatchModeConfig {
                enabled: true,
                debounce_ms: 2000,
            },
            generation: GenerationConfig {
                max_suggestions: 3,
                include_security: true,
                include_negative_paths: true,
            },
        }
    }
}
