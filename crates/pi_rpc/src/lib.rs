//! pi RPC protocol types.
//!
//! Type-safe Rust representation of the pi coding agent RPC protocol.
//!
//! # Quick Start
//!
//! ```rust
//! use pi_rpc::command::Command;
//! use pi_rpc::event::Event;
//!
//! // Create a command
//! let cmd = Command::prompt("Hello!").with_id("req-1");
//! let line = r#"{"type":"prompt","message":"Hello!","id":"req-1"}"#.to_string();
//! assert_eq!(cmd.to_json_line(), line + "\n");
//!
//! // Parse an event
//! let event = Event::from_json_line(r#"{"type":"agent_start"}"#).unwrap();
//! assert!(matches!(event, Event::AgentStart));
//! ```

pub mod command;
pub mod event;
pub mod ext;
pub mod message;
pub mod model;
