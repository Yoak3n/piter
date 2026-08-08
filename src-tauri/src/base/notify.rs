//! 会话完成系统通知（0.2.0 P3）。
//!
//! 分层策略：
//! - 窗口可见且聚焦 → 前端顶部 toast 已覆盖，不发系统通知（避免打扰）。
//! - 窗口最小化 / 失焦 / 关闭到托盘 → OS 系统通知（托盘隐藏时前端 WS 断开，
//!   系统通知是唯一可达通道，必须走 Rust 侧）。
//! - 发送失败（如 Linux 无通知守护进程）→ 静默降级，不影响主流程。

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::base::window::{
    manager::Manager as WM,
    schema::{WindowState, WindowType},
};

/// 简单去重守卫：记录最近一次通知的 (unix_secs, instance_id 摘要)。
/// agent_end 正常一条会话事件只发一次，但为防广播/重连双发，同秒内同会话跳过。
static LAST_NOTIFY: Mutex<Option<(u64, u64)>> = Mutex::new(None);

/// 向 gateway 注册 agent_end 观察回调。
/// 回调运行在 gateway 事件循环线程，内部只做窗口状态判断 + 发通知，必须快速返回。
pub fn init(app: &AppHandle, gw: &Arc<pi_server::gateway::GatewayState>) {
    let handle = app.clone();
    gw.set_agent_end_hook(move |instance_id: &str, session_label: &str| {
        notify_session_completed(&handle, instance_id, session_label);
    });
}

fn notify_session_completed(app: &AppHandle, instance_id: &str, session_label: &str) {
    // 窗口可见且聚焦：前端 toast 已覆盖，不发系统通知。
    if matches!(
        WM::global().get_cached_window_state(WindowType::Main),
        WindowState::VisibleFocused
    ) {
        return;
    }

    // 同秒去重（防 agent_end 双发）。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let id_hash = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        instance_id.hash(&mut h);
        h.finish()
    };
    {
        let mut last = LAST_NOTIFY.lock();
        if last
            .map(|(secs, h)| secs == now && h == id_hash)
            .unwrap_or(false)
        {
            return;
        }
        *last = Some((now, id_hash));
    }

    // label 为空（自动命名前的极早期会话）→ 回退 instance_id 前 8 位。
    let title = if session_label.is_empty() {
        format!("Piter · {}", &instance_id[..instance_id.len().min(8)])
    } else {
        session_label.to_string()
    };

    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body("已完成")
        .show()
    {
        // Linux 无通知守护（headless/精简桌面）等场景：静默降级，不影响主流程。
        log::debug!("[notify] session-complete notification failed: {}", e);
    }
}
