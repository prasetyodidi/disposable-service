use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatusResponse {
    pub is_claimed: bool,
    pub is_authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPasswordRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub login_time: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessionsResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PandocConvertRequest {
    pub markdown: String,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<T>,
}
