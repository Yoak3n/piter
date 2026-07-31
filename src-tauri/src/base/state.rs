use std::sync::Arc;
use parking_lot::Mutex;

use crate::base::lightweight::LightWeightState;
#[derive(Clone)]
pub struct AppState {
    pub lightweight: Arc<Mutex<LightWeightState>>,
    pub pi_version: String,
    /// Gateway HTTP URL, e.g. "http://127.0.0.1:10041/"
    pub web_url: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            lightweight: Arc::new(Mutex::new(LightWeightState::default())),
            pi_version: String::new(),
            web_url: String::new(),
        }
    }
}