use crate::base::window::{manager::Manager as WM, schema::WindowType};

// ─── 窗口控制命令 ─────────────────────────────────────────────────────
// 前端标题栏的所有窗口操作统一走这里（invoke → WindowManager），
// 不再直接调 @tauri-apps/api/window 原生方法——保证 WM 的缓存状态
// （WindowState）与实际窗口状态一致，托盘 toggle / 轻量模式判断不脱节。

/// 最小化主窗口（走 WindowManager，缓存状态同步为 Minimized）。
#[tauri::command]
pub fn minimize_window() -> Result<(), String> {
    if WM::global().minimized_window(WindowType::Main) {
        Ok(())
    } else {
        Err("Main window not found".into())
    }
}

/// 最大化 / 还原主窗口（走 WindowManager）。
#[tauri::command]
pub fn toggle_maximize_window() -> Result<(), String> {
    if WM::global().toggle_maximize_window(WindowType::Main) {
        Ok(())
    } else {
        Err("Main window not found".into())
    }
}

/// 关闭（隐藏到托盘）主窗口。
/// 走 WM::close_window → window.close() → CloseRequested（init.rs）
/// → prevent_close + hide + 同步缓存为 Hidden。
#[tauri::command]
pub fn close_window() -> Result<(), String> {
    let _ = WM::global().close_window(WindowType::Main);
    Ok(())
}

/// 查询主窗口是否最大化（标题栏最大化图标状态）。
#[tauri::command]
pub fn is_maximized_window() -> Result<bool, String> {
    WM::global()
        .is_maximized_window(WindowType::Main)
        .ok_or_else(|| "Main window not found".into())
}

/// 显示并聚焦主窗口（如需要）。
#[tauri::command]
pub fn show_window() -> Result<(), String> {
    match WM::global().show_window(WindowType::Main, None) {
        crate::base::window::schema::WindowOperationResult::Failed => {
            Err("Failed to show window".into())
        }
        _ => Ok(()),
    }
}
