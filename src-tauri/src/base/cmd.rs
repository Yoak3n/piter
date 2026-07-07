/// Return the broker HTTP URL (for desktop frontend to discover).
#[tauri::command]
pub fn get_broker_url() -> String {
    std::env::var("PI_HTTP_URL").unwrap_or_else(|_| "http://127.0.0.1:0/".to_string())
}
