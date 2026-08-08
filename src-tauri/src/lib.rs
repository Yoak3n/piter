pub mod admin;
pub mod base;
pub mod pi;
// D3: Linux（AUR 场景）关闭内置 updater 模块，避免未使用代码告警。
#[cfg(not(target_os = "linux"))]
pub mod updater;
use base::init;
pub use base::window::manager::Manager as WM;

/// Return the bundle identifier from tauri.conf.json (e.g. `com.yoa.piter`).
/// This is also the name of the app data directory under `%APPDATA%`, which
/// the NSIS uninstaller's built-in "delete app data" option removes.
pub fn identifier() -> String {
    let ctx: tauri::Context<tauri::Wry> = tauri::generate_context!();
    ctx.config().identifier.clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init::configure(tauri::Builder::default())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(init::generate_handlers())
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(base::init::app_event_handle);
}