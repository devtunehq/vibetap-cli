//! Configuration management for VibeTap
//!
//! Handles loading and saving configuration from:
//! - Global config: ~/.config/vibetap/config.toml
//! - Project config: .vibetap/config.json

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Read(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(String),

    #[error("Not authenticated. Run 'vibetap auth login' first.")]
    NotAuthenticated,

    #[error("Failed to refresh token: {0}")]
    RefreshFailed(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
}

/// Authentication tokens (OAuth or API key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub auth_type: String, // "oauth" or "api_key"
}

/// Global configuration (stored in ~/.config/vibetap/)
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    pub api_url: Option<String>,
    pub tokens: Option<AuthTokens>,
}

/// Project-level configuration (stored in .vibetap/)
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
    pub tokens: Option<AuthTokens>,
}

impl Config {
    /// Load configuration from all sources
    pub fn load() -> Result<Self, ConfigError> {
        let global = Self::load_global()?;
        let project = Self::load_project().ok();
        let tokens = global.tokens.clone();

        Ok(Self { global, project, tokens })
    }

    /// Get the API URL (with default fallback)
    pub fn api_url(&self) -> &str {
        self.global
            .api_url
            .as_deref()
            .unwrap_or("https://vibetap.dev")
    }

    /// Get the access token (or error if not authenticated)
    pub fn access_token(&self) -> Result<&str, ConfigError> {
        self.tokens
            .as_ref()
            .map(|t| t.access_token.as_str())
            .ok_or(ConfigError::NotAuthenticated)
    }

    /// Check if authenticated
    pub fn is_authenticated(&self) -> bool {
        self.tokens.is_some()
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
        let path = Path::new(".vibetap/config.json");

        if !path.exists() {
            return Err(ConfigError::Read(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Project config not found",
            )));
        }

        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Save authentication tokens
    pub fn save_tokens(tokens: &AuthTokens, api_url: &str) -> Result<(), ConfigError> {
        let dir = Self::global_config_dir();
        std::fs::create_dir_all(&dir)?;

        let config = GlobalConfig {
            api_url: Some(api_url.to_string()),
            tokens: Some(tokens.clone()),
        };

        let path = Self::global_config_path();
        let content = toml::to_string_pretty(&config).map_err(|e| ConfigError::Parse(e.to_string()))?;
        std::fs::write(path, content)?;

        Ok(())
    }

    /// Clear authentication tokens (logout)
    pub fn clear_tokens() -> Result<(), ConfigError> {
        let path = Self::global_config_path();

        if path.exists() {
            let config = GlobalConfig {
                api_url: None,
                tokens: None,
            };

            let content = toml::to_string_pretty(&config).map_err(|e| ConfigError::Parse(e.to_string()))?;
            std::fs::write(path, content)?;
        }

        Ok(())
    }

    /// Check if the current OAuth token is expired or about to expire
    pub fn is_token_expired(&self) -> bool {
        match &self.tokens {
            Some(tokens) => {
                // API keys don't expire (in the same way)
                if tokens.auth_type == "api_key" {
                    return false;
                }

                // Check expiration with 5 minute buffer
                match tokens.expires_at {
                    Some(expires_at) => {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        expires_at - 300 < now // 5 minute buffer
                    }
                    None => false, // No expiry set, assume valid
                }
            }
            None => true, // No tokens = effectively expired
        }
    }

    /// Get a valid access token, refreshing if necessary
    pub async fn get_valid_access_token(&mut self) -> Result<String, ConfigError> {
        if !self.is_authenticated() {
            return Err(ConfigError::NotAuthenticated);
        }

        // Check if we need to refresh
        if self.is_token_expired() {
            self.refresh_access_token().await?;
        }

        self.access_token().map(|s| s.to_string())
    }

    /// Refresh the access token using the refresh token
    pub async fn refresh_access_token(&mut self) -> Result<(), ConfigError> {
        let tokens = self.tokens.as_ref().ok_or(ConfigError::NotAuthenticated)?;

        // API keys don't need refresh
        if tokens.auth_type == "api_key" {
            return Ok(());
        }

        let refresh_token = tokens.refresh_token.as_ref().ok_or_else(|| {
            ConfigError::RefreshFailed("No refresh token available".to_string())
        })?;

        let api_url = self.api_url().to_string();
        let url = format!("{}/api/v1/auth/refresh", api_url);

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "refresh_token": refresh_token
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            // If refresh token is invalid/expired, clear tokens so user can re-auth
            if status == reqwest::StatusCode::UNAUTHORIZED
                || body.contains("Already Used")
                || body.contains("Invalid Refresh Token")
                || body.contains("expired")
            {
                let _ = Self::clear_tokens();
                return Err(ConfigError::RefreshFailed(
                    "Session expired. Please run 'vibetap auth login' to re-authenticate.".to_string()
                ));
            }

            return Err(ConfigError::RefreshFailed(format!(
                "Server returned {}: {}",
                status, body
            )));
        }

        let refresh_response: RefreshResponse = response
            .json()
            .await
            .map_err(|e| ConfigError::RefreshFailed(format!("Failed to parse response: {}", e)))?;

        if !refresh_response.success {
            let msg = refresh_response
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown error".to_string());

            // Clear tokens on auth failures
            if msg.contains("Already Used") || msg.contains("Invalid") || msg.contains("expired") {
                let _ = Self::clear_tokens();
                return Err(ConfigError::RefreshFailed(
                    "Session expired. Please run 'vibetap auth login' to re-authenticate.".to_string()
                ));
            }

            return Err(ConfigError::RefreshFailed(msg));
        }

        let data = refresh_response.data.ok_or_else(|| {
            ConfigError::RefreshFailed("No token data in response".to_string())
        })?;

        // Update tokens
        let new_tokens = AuthTokens {
            access_token: data.access_token,
            refresh_token: Some(data.refresh_token),
            expires_at: Some(data.expires_at),
            auth_type: "oauth".to_string(),
        };

        // Save to disk
        Self::save_tokens(&new_tokens, &api_url)?;

        // Update in-memory config
        self.tokens = Some(new_tokens.clone());
        self.global.tokens = Some(new_tokens);

        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
struct RefreshResponse {
    success: bool,
    data: Option<RefreshData>,
    error: Option<RefreshError>,
}

#[derive(Debug, serde::Deserialize)]
struct RefreshData {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

#[derive(Debug, serde::Deserialize)]
struct RefreshError {
    message: String,
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
