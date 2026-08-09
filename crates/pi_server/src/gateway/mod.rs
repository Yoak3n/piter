//! Gateway module — HTTP+WS server, client management, message routing.
//!
//! 做什么：坐在 UI 客户端与 broker 之间，提供 WebSocket 连接/事件广播、
//! REST API（health/sessions/pi control）、会话生命周期管理与路由表维护。
//! 不做什么：不直接与 pi 进程通信（那是 broker 的职责）。
//! 依赖：上层（src-tauri / lib.rs）通过 `GatewayState` 与 `start_gateway` 使用。
//!
//! 布局（按生命周期阶段分文件）：
//! - state.rs      GatewayState 结构体 + 生命周期方法 + 项目树构建
//! - server.rs     start_gateway：端口绑定、Router 构建、服务线程 spawn
//! - event_loop.rs run_event_loop + process_broker_event + track_and_broadcast
//! - responses.rs  handle_response_event 及三个子 handler（session/get_state/model）
//! - broadcast.rs  WS 广播 / sessions_list 推送
//! - db.rs         SQLite 数据层（会话/项目/扩展/搜索/设置，A2 后按领域分文件）
//! - ws/           WebSocket handler 与命令路由
//! - handlers/     薄 HTTP 路由层（各领域一个文件）
//! - session_manager.rs / project.rs / ext_cache.rs / lan_auth.rs  领域逻辑

mod broadcast;
pub mod db;
pub mod ext_cache;
mod event_loop;
pub mod handlers;
mod helper;
mod lan_auth;
mod messages;
pub mod project;
mod responses;
pub mod server;
pub mod session_manager;
pub mod state;
pub mod ws;

pub use state::GatewayState;

// 兼容别名：以下符号被子模块通过 `super::` 引用（历史路径，随后续拆分逐步收敛为直接导入）。
use crate::broker::types::PROTOCOL_VERSION;
use broadcast::push_sessions_list_to_clients;
use messages::command;
