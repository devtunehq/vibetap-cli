//! VibeTap Core Library
//!
//! Core functionality for VibeTap including:
//! - API client for communicating with VibeTap SaaS
//! - Configuration management
//! - Diff processing

pub mod api;
pub mod config;

pub use api::ApiClient;
pub use config::{Config, GlobalConfig};
