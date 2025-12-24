//! VibeTap API Client
//!
//! Handles communication with the VibeTap SaaS API.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error: {code} - {message}")]
    Api { code: String, message: String },

    #[error("Unauthorized: Invalid or expired API key")]
    Unauthorized,

    #[error("Rate limited: retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },

    #[error("Quota exceeded")]
    QuotaExceeded,
}

/// API client for VibeTap SaaS
pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

/// Request to generate tests
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub diff: DiffPayload,
    pub context: Vec<FileContext>,
    pub options: GenerateOptions,
    pub policy_pack_id: Option<String>,
    pub repo_identifier: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPayload {
    pub hunks: Vec<DiffHunk>,
    pub base_branch: Option<String>,
    pub head_commit: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub file_path: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContext {
    pub path: String,
    pub content: String,
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOptions {
    pub test_runner: String,
    pub max_suggestions: u32,
    pub include_security: bool,
    pub include_negative_paths: bool,
    pub model_tier: String,
}

/// Response from generate endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResponse {
    pub suggestions: Vec<TestSuggestion>,
    pub summary: String,
    pub model_used: String,
    #[serde(default)]
    pub used_byok: bool,
    pub tokens_used: u32,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSuggestion {
    pub id: String,
    pub file_path: String,
    pub test_runner: String,
    pub code: String,
    pub description: String,
    pub category: String,
    pub confidence: f64,
    pub runtime_estimate: String,
    pub risks_addressed: Vec<String>,
}

/// API response wrapper
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiErrorResponse>,
    pub meta: ResponseMeta,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorResponse {
    pub code: String,
    pub message: String,
    pub retry_after: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMeta {
    pub request_id: String,
    pub tokens_used: Option<u32>,
    pub timestamp: String,
}

impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    /// Generate test suggestions from a diff
    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, ApiError> {
        let url = format!("{}/api/v1/generate", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);
            return Err(ApiError::RateLimited { retry_after });
        }

        let response_text = response.text().await?;

        let api_response: ApiResponse<GenerateResponse> = serde_json::from_str(&response_text)
            .map_err(|e| ApiError::Api {
                code: "PARSE_ERROR".to_string(),
                message: format!("Failed to parse response: {}. Body: {}", e, &response_text[..response_text.len().min(500)]),
            })?;

        if !api_response.success {
            if let Some(error) = api_response.error {
                if error.code == "QUOTA_EXCEEDED" {
                    return Err(ApiError::QuotaExceeded);
                }
                return Err(ApiError::Api {
                    code: error.code,
                    message: error.message,
                });
            }
        }

        api_response
            .data
            .ok_or_else(|| ApiError::Api {
                code: "NO_DATA".to_string(),
                message: "Response contained no data".to_string(),
            })
    }

    /// Query current usage
    pub async fn get_usage(&self) -> Result<UsageResponse, ApiError> {
        let url = format!("{}/api/v1/usage", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }

        let api_response: ApiResponse<UsageResponse> = response.json().await?;

        api_response
            .data
            .ok_or_else(|| ApiError::Api {
                code: "NO_DATA".to_string(),
                message: "Response contained no data".to_string(),
            })
    }

    /// Get user stats for the stats command
    pub async fn get_stats(&self) -> Result<StatsResponse, ApiError> {
        let url = format!("{}/api/v1/stats", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }

        let api_response: ApiResponse<StatsResponse> = response.json().await?;

        api_response
            .data
            .ok_or_else(|| ApiError::Api {
                code: "NO_DATA".to_string(),
                message: "Response contained no data".to_string(),
            })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    pub period: UsagePeriod,
    pub usage: UsageDetails,
    pub limits: UsageLimits,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePeriod {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDetails {
    pub total_requests: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimits {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub tokens_per_day: u32,
    pub tokens_remaining: u32,
}

/// Stats response from the stats endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub this_month: MonthlyStats,
    pub all_time: AllTimeStats,
    pub plan: PlanInfo,
    #[serde(default)]
    pub byok: Option<ByokInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByokInfo {
    pub enabled: bool,
    pub total_requests: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyStats {
    pub generations: u32,
    pub remaining: u32,
    pub limit: u32,
    pub security_issues_caught: u32,
    pub tests_applied: u32,
    pub acceptance_rate: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllTimeStats {
    pub total_generations: u32,
    pub total_security_issues: u32,
    pub total_tests_applied: u32,
    pub top_framework: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInfo {
    pub name: String,
    pub generations_per_month: u32,
    pub credits_balance: u32,
}
