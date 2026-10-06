use chrono::Utc;
use disposable_common::SessionInfo;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub password_hash: Arc<RwLock<Option<String>>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    pub jwt_secret: Vec<u8>,
}

impl AppState {
    pub fn new() -> Self {
        use rand::RngCore;
        let mut secret = vec![0u8; 64];
        rand::thread_rng().fill_bytes(&mut secret);

        Self {
            password_hash: Arc::new(RwLock::new(None)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret: secret,
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
        let now = Utc::now();
        lock.retain(|_, s| s.expires_at > now);
        lock.insert(session.session_id.clone(), session);
    }

    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let mut lock = self.sessions.write().await;
        let now = Utc::now();
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
        let now = Utc::now();
        lock.retain(|_, s| s.expires_at > now);
        let mut list: Vec<SessionInfo> = lock.values().cloned().collect();
        list.sort_by(|a, b| b.login_time.cmp(&a.login_time));
        list
    }
}
