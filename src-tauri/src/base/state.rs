use std::sync::Arc;
use parking_lot::Mutex;

use crate::base::lightweight::LightWeightState;
#[derive(Clone)]
pub struct AppState {
    pub lightweight: Arc<Mutex<LightWeightState>>,
    pub pi_version: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            lightweight: Arc::new(Mutex::new(LightWeightState::default())),
            pi_version: String::new(),
        }
    }
}