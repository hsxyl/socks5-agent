use dashmap::DashMap;
use std::sync::Arc;
use yamux::Control;

#[derive(Clone, Default)]
pub struct SessionManager {
    sessions: Arc<DashMap<String, Control>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, device_id: String, control: Control) {
        self.sessions.insert(device_id, control);
    }

    pub fn unregister(&self, device_id: &str) {
        self.sessions.remove(device_id);
    }

    pub fn get_available_control(&self) -> Option<(String, Control)> {
        if self.sessions.is_empty() {
            return None;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as usize;
        let skip = now % self.sessions.len();
        self.sessions.iter().skip(skip).map(|entry| (entry.key().clone(), entry.value().clone())).next()
    }

    pub fn get_specific_control(&self, device_id: &str) -> Option<(String, Control)> {
        self.sessions.get(device_id).map(|entry| (entry.key().clone(), entry.value().clone()))
    }
}
