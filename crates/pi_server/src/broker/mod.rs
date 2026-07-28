//! Broker module — pi process types, spawning, and utilities.
//!
//! The broker does NOT run any server. It provides:
//! - Types for pi process instances and shared state
//! - `SpawnBuilder` for spawning pi child processes
//! - Utility functions for paths, environment, and settings
//!
//! The gateway module owns the HTTP/WS server and uses the broker to manage pi processes.

pub mod process;
pub mod types;
pub mod util;

pub use types::PiAgentSettings;
pub use util::{get_pi_agent_dir, piter_data_dir, read_pi_settings};
