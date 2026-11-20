#![recursion_limit = "512"]

pub mod config;
pub mod conversation;
pub mod orchestrator;
pub mod telemetry;
pub mod registry;
pub mod hydration;
pub mod overview;
pub mod theme;
pub mod kopeng;
pub mod logger;
pub mod mcp;
pub mod caller_identity;
pub mod api;
pub mod brief;
pub mod db;
pub mod desktop;
pub mod watcher;
pub mod events;
pub mod supervisor;
pub mod duck;
pub mod port_inspect;
pub mod semantic;
pub mod ui_state;
pub mod capabilities;
pub mod voice_harness;
pub mod voice_controller;

/// Identifier of this compiled library (epoch seconds of the build script run, see `build.rs`).
/// The desktop app and the standalone daemon share it only when built from the same sources.
pub const BUILD_ID: &str = env!("ONESHIP_BUILD_ID");

static RESOLVED_RENDERER_INFO: std::sync::OnceLock<(String, String)> = std::sync::OnceLock::new();

/// Records the WebKitGTK renderer mode and origin resolved in `main()` so `run()` can log it.
pub fn set_resolved_renderer_info(mode: &str, origin: &str) {
    let _ = RESOLVED_RENDERER_INFO.set((mode.to_string(), origin.to_string()));
}

/// Returns [`BUILD_ID`] of the desktop app so the UI can compare it with the daemon's.
#[tauri::command]
fn get_build_id_cmd() -> String {
    BUILD_ID.to_string()
}

/// Port of the MCP daemon (`mcp.mcp_daemon_port`), the single source for every daemon URL in the UI.
#[tauri::command]
fn get_daemon_port_cmd() -> u16 {
    crate::config::mcp_daemon_port()
}

/// The desktop app: a window over the daemon (spec 12 §7). It opens no database, supervises
/// nothing and serves no domain command; the webview calls the daemon directly. Its only Tauri
/// commands bootstrap that connection, plus the native dialog and URL opener plugins.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Closing the window ends the interface process; the daemon keeps every process and all
        // state (spec 12 §7.3), so nothing is kept alive in a hidden window.
        .setup(|_app| {
            logger::use_file_sink_only();
            if let Some((mode, origin)) = RESOLVED_RENDERER_INFO.get() {
                let level = if origin == "launcher retry" { "WARN" } else { "INFO" };
                logger::log(
                    level,
                    "core",
                    &format!("WebKitGTK renderer mode: {} (origin: {})", mode, origin),
                );
            }
            logger::log("INFO", "core", "ONE-SHIP interface initialized (GUI Mode)");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_build_id_cmd, get_daemon_port_cmd])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
