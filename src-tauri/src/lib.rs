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
        .setup(|app| {
            logger::use_file_sink_only();
            if let Some((mode, origin)) = RESOLVED_RENDERER_INFO.get() {
                let level = if origin == "launcher retry" { "WARN" } else { "INFO" };
                logger::log(
                    level,
                    "core",
                    &format!("WebKitGTK renderer mode: {} (origin: {})", mode, origin),
                );
            }
            if !crate::desktop::is_hyprland_session() {
                setup_desktop_tray(app);
            }
            logger::log("INFO", "core", "ONE-SHIP interface initialized (GUI Mode)");
            Ok(())
        })
        .on_window_event(|window, event| {
            if !crate::desktop::is_hyprland_session() {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        logger::log("WARN", "core", &format!("Failed to hide main window on close: {e}"));
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![get_build_id_cmd, get_daemon_port_cmd])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn focus_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        if let Err(e) = win.unminimize() {
            logger::log("WARN", "core", &format!("Tray unminimize failed: {e}"));
        }
        if let Err(e) = win.show() {
            logger::log("WARN", "core", &format!("Tray show failed: {e}"));
        }
        if let Err(e) = win.set_focus() {
            logger::log("WARN", "core", &format!("Tray focus failed: {e}"));
        }
    }
}

fn hide_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        if let Err(e) = win.hide() {
            logger::log("WARN", "core", &format!("Tray hide failed: {e}"));
        }
    }
}

fn run_daemon_systemctl(action: &str) -> bool {
    match std::process::Command::new("systemctl")
        .args(["--user", action, "oneship-daemon.service"])
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

fn stop_daemon_and_exit(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        if !run_daemon_systemctl("stop") {
            let _ = std::process::Command::new("pkill")
                .args(["-TERM", "-x", "oneship-daemon"])
                .status();
        }
        app.exit(0);
    });
}

fn setup_desktop_tray(app: &tauri::App) {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show_item = match MenuItem::with_id(app, "show", "Open ONE-SHIP", true, None::<&str>) {
        Ok(item) => item,
        Err(e) => {
            logger::log("WARN", "core", &format!("Tray menu init failed: {e}"));
            return;
        }
    };
    let hide_item = match MenuItem::with_id(app, "hide", "Close Interface", true, None::<&str>) {
        Ok(item) => item,
        Err(e) => {
            logger::log("WARN", "core", &format!("Tray hide item init failed: {e}"));
            return;
        }
    };
    let restart_item = match MenuItem::with_id(app, "restart_daemon", "Restart Daemon", true, None::<&str>) {
        Ok(item) => item,
        Err(e) => {
            logger::log("WARN", "core", &format!("Tray restart item init failed: {e}"));
            return;
        }
    };
    let sep = match PredefinedMenuItem::separator(app) {
        Ok(item) => item,
        Err(e) => {
            logger::log("WARN", "core", &format!("Tray separator init failed: {e}"));
            return;
        }
    };
    let quit_item = match MenuItem::with_id(app, "quit", "Quit (Stop Daemon)", true, None::<&str>) {
        Ok(item) => item,
        Err(e) => {
            logger::log("WARN", "core", &format!("Tray quit item init failed: {e}"));
            return;
        }
    };
    let menu = match Menu::with_items(app, &[&show_item, &hide_item, &restart_item, &sep, &quit_item]) {
        Ok(m) => m,
        Err(e) => {
            logger::log("WARN", "core", &format!("Tray menu build failed: {e}"));
            return;
        }
    };

    let mut builder = TrayIconBuilder::with_id("one-ship-tray")
        .menu(&menu)
        .tooltip("ONE-SHIP")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => focus_main_window(app),
            "hide" => hide_main_window(app),
            "restart_daemon" => {
                std::thread::spawn(|| {
                    if !run_daemon_systemctl("restart") {
                        logger::log("WARN", "core", "Tray failed to restart oneship-daemon.service");
                    }
                });
            }
            "quit" => stop_daemon_and_exit(app.clone()),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    if let Err(e) = builder.build(app) {
        logger::log("WARN", "core", &format!("Desktop tray icon unavailable: {e}"));
    }
}
