use chrono::{DateTime, Duration, Utc};
use crate::privacy::burn::BurnCoordinator;
use disposable_common::{AuditReport, SessionInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Root directory for all temporary workspace files (e.g. pandoc jobs).
/// Burn verification scans this directory.
pub fn workspace_tmp_root() -> std::path::PathBuf {
    std::env::temp_dir().join("disposable-workspace")
}

#[derive(Clone, Debug)]
pub struct EphemeralDownload {
    pub data: Vec<u8>,
    pub filename: String,
    pub content_type: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct AppState {
    pub password_hash: Arc<RwLock<Option<String>>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    pub jwt_secret: Vec<u8>,
    pub downloads: Arc<RwLock<HashMap<String, EphemeralDownload>>>,
    pub mcp_sessions: Arc<RwLock<HashMap<String, Sender<axum::response::sse::Event>>>>,
    pub burn: Arc<BurnCoordinator>,
    pub ui_events: broadcast::Sender<Arc<AuditReport>>,
}

impl AppState {
    pub fn new() -> Self {
        let initial_hash = std::env::var("DISPOSABLE_PASSWORD_HASH")
            .ok()
            .and_then(|h| {
                let trimmed = h.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

        let jwt_secret: Vec<u8> = if let Ok(s) = std::env::var("JWT_SECRET") {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                trimmed.as_bytes().to_vec()
            } else {
                Self::derive_jwt_secret(&initial_hash)
            }
        } else {
            Self::derive_jwt_secret(&initial_hash)
        };

        Self {
            password_hash: Arc::new(RwLock::new(initial_hash)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret,
            downloads: Arc::new(RwLock::new(HashMap::new())),
            mcp_sessions: Arc::new(RwLock::new(HashMap::new())),
            burn: Arc::new(BurnCoordinator::new()),
            ui_events: broadcast::channel(4).0,
        }
    }

    fn derive_jwt_secret(initial_hash: &Option<String>) -> Vec<u8> {
        if let Some(hash) = initial_hash {
            use sha2::{Digest, Sha256};
            let mut h1 = Sha256::new();
            h1.update(b"disposable-service-salt-a:");
            h1.update(hash.as_bytes());
            let part1 = h1.finalize();

            let mut h2 = Sha256::new();
            h2.update(b"disposable-service-salt-b:");
            h2.update(hash.as_bytes());
            let part2 = h2.finalize();

            let mut secret = Vec::with_capacity(64);
            secret.extend_from_slice(&part1);
            secret.extend_from_slice(&part2);
            secret
        } else {
            use rand::RngCore;
            let mut secret = vec![0u8; 64];
            rand::thread_rng().fill_bytes(&mut secret);
            secret
        }
    }

    pub async fn is_claimed(&self) -> bool {
        self.password_hash.read().await.is_some()
    }

    pub async fn set_password_hash(&self, hash: String) -> Result<(), &'static str> {
        let mut lock = self.password_hash.write().await;
        if lock.is_some() {
            return Err("Password has already been set");
        }
        *lock = Some(hash);
        Ok(())
    }

    pub async fn add_session(&self, session: SessionInfo) {
        let mut lock = self.sessions.write().await;
        // Purge expired sessions
        let now = Utc::now().timestamp();
        lock.retain(|_, s| s.expires_at > now);
        lock.insert(session.session_id.clone(), session);
    }

    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let mut lock = self.sessions.write().await;
        let now = Utc::now().timestamp();
        if let Some(session) = lock.get(session_id) {
            if session.expires_at > now {
                return Some(session.clone());
            } else {
                lock.remove(session_id);
            }
        }
        None
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut lock = self.sessions.write().await;
        lock.remove(session_id);
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut lock = self.sessions.write().await;
        let now = Utc::now().timestamp();
        lock.retain(|_, s| s.expires_at > now);
        let mut list: Vec<SessionInfo> = lock.values().cloned().collect();
        list.sort_by(|a, b| b.login_time.cmp(&a.login_time));
        list
    }

    pub async fn store_ephemeral_download(
        &self,
        data: Vec<u8>,
        filename: String,
        content_type: String,
        ttl_secs: u64,
    ) -> String {
        let token = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + Duration::seconds(ttl_secs as i64);
        let mut lock = self.downloads.write().await;
        // Purge expired downloads
        let now = Utc::now();
        lock.retain(|_, d| d.expires_at > now);
        lock.insert(
            token.clone(),
            EphemeralDownload {
                data,
                filename,
                content_type,
                expires_at,
            },
        );
        token
    }

    pub async fn take_ephemeral_download(&self, token: &str) -> Option<EphemeralDownload> {
        let mut lock = self.downloads.write().await;
        if let Some(download) = lock.remove(token) {
            if download.expires_at > Utc::now() {
                return Some(download);
            }
        }
        None
    }

    pub async fn add_mcp_session(&self, session_id: String, sender: Sender<axum::response::sse::Event>) {
        let mut lock = self.mcp_sessions.write().await;
        lock.insert(session_id, sender);
    }

    pub async fn send_mcp_message(&self, session_id: &str, event: axum::response::sse::Event) -> Result<(), String> {
        let sender = {
            let lock = self.mcp_sessions.read().await;
            lock.get(session_id).cloned()
        };
        if let Some(tx) = sender {
            tx.send(event).await.map_err(|e| format!("Failed to send SSE event: {}", e))
        } else {
            Err(format!("MCP session {} not found", session_id))
        }
    }

    pub async fn remove_mcp_session(&self, session_id: &str) {
        let mut lock = self.mcp_sessions.write().await;
        lock.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ephemeral_download_store_and_take() {
        let state = AppState::new();
        let data = b"Hello, ephemeral PDF!".to_vec();
        let filename = "sample.pdf".to_string();
        let content_type = "application/pdf".to_string();

        let token = state
            .store_ephemeral_download(data.clone(), filename.clone(), content_type.clone(), 60)
            .await;

        assert!(!token.is_empty());

        // First take -> Some(data)
        let download = state.take_ephemeral_download(&token).await;
        assert!(download.is_some());
        let d = download.unwrap();
        assert_eq!(d.data, data);
        assert_eq!(d.filename, filename);
        assert_eq!(d.content_type, content_type);

        // Second take (burn-after-reading) -> None
        let second_take = state.take_ephemeral_download(&token).await;
        assert!(second_take.is_none());
    }

    #[tokio::test]
    async fn test_ephemeral_download_expired() {
        let state = AppState::new();
        let data = b"Expired content".to_vec();
        let filename = "expired.pdf".to_string();
        let content_type = "application/pdf".to_string();

        // Expired download: insert manually with past expiry
        let token = Uuid::new_v4().to_string();
        {
            let mut lock = state.downloads.write().await;
            lock.insert(
                token.clone(),
                EphemeralDownload {
                    data,
                    filename,
                    content_type,
                    expires_at: Utc::now() - Duration::seconds(10),
                },
            );
        }

        let download = state.take_ephemeral_download(&token).await;
        assert!(download.is_none());
    }

    #[tokio::test]
    async fn test_mcp_session_lifecycle() {
        let state = AppState::new();
        let session_id = "mcp-test-session-1".to_string();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);

        state.add_mcp_session(session_id.clone(), tx).await;

        let event = axum::response::sse::Event::default().data("test message");
        let send_res = state.send_mcp_message(&session_id, event).await;
        assert!(send_res.is_ok());

        let received = rx.recv().await;
        assert!(received.is_some());

        state.remove_mcp_session(&session_id).await;
        let event2 = axum::response::sse::Event::default().data("test message 2");
        let send_res2 = state.send_mcp_message(&session_id, event2).await;
        assert!(send_res2.is_err());
    }

    #[tokio::test]
    async fn test_master_password_env_injection() {
        let dummy_hash = "$argon2id$v=19$m=19456,t=2,p=1$dummyhashval";
        std::env::set_var("DISPOSABLE_PASSWORD_HASH", dummy_hash);
        let state = AppState::new();
        assert!(state.is_claimed().await);

        // Setting password again must fail
        let res = state.set_password_hash("new_hash".to_string()).await;
        assert!(res.is_err());

        std::env::remove_var("DISPOSABLE_PASSWORD_HASH");
        let fresh_state = AppState::new();
        assert!(!fresh_state.is_claimed().await);
    }

    #[tokio::test]
    async fn test_deterministic_jwt_secret() {
        // Test explicit JWT_SECRET
        std::env::set_var("JWT_SECRET", "my_custom_lambda_secret_123");
        let s1 = AppState::new();
        let s2 = AppState::new();
        assert_eq!(s1.jwt_secret, b"my_custom_lambda_secret_123");
        assert_eq!(s1.jwt_secret, s2.jwt_secret);
        std::env::remove_var("JWT_SECRET");

        // Test hash-derived deterministic secret
        std::env::set_var("DISPOSABLE_PASSWORD_HASH", "argon2hash_value_xyz");
        let d1 = AppState::new();
        let d2 = AppState::new();
        assert_eq!(d1.jwt_secret.len(), 64);
        assert_eq!(d1.jwt_secret, d2.jwt_secret);
        std::env::remove_var("DISPOSABLE_PASSWORD_HASH");
    }
}
