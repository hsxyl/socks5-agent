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

    pub fn get_available_control(&self) -> Option<Control> {
        // For MVP, just get the first available session
        self.sessions.iter().map(|entry| entry.value().clone()).next()
    }
}
