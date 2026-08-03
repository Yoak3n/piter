pub mod admin;
pub mod base;
pub mod pi;
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
        .invoke_handler(init::generate_handlers())
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(base::init::app_event_handle);
}