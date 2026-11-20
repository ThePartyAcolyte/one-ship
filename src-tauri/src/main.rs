#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_webkit_renderer_mode() {
    use oneship_lib::config::{read_webkit_renderer_mode, WebkitRendererMode};

    let configured = read_webkit_renderer_mode();
    let launcher_retry = std::env::var("ONESHIP_WEBKIT_LAUNCHER_RETRY").ok().as_deref() == Some("1");
    let (resolved, origin) = if launcher_retry && !matches!(configured, WebkitRendererMode::Software) {
        (WebkitRendererMode::SharedMemory, "launcher retry")
    } else {
        match configured {
            WebkitRendererMode::Auto => {
                if std::path::Path::new("/proc/driver/nvidia/version").exists() {
                    (WebkitRendererMode::SharedMemory, "auto (nvidia)")
                } else if std::fs::read_dir("/dev/dri").is_err() {
                    (WebkitRendererMode::SharedMemory, "auto (no-dri)")
                } else {
                    (WebkitRendererMode::Dmabuf, "auto (dri)")
                }
            }
            explicit => (explicit, "config"),
        }
    };

    match resolved {
        WebkitRendererMode::Dmabuf | WebkitRendererMode::Auto => {}
        WebkitRendererMode::SharedMemory => {
            std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
        }
        WebkitRendererMode::Software => {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    oneship_lib::set_resolved_renderer_info(resolved.as_str(), origin);
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        configure_webkit_renderer_mode();
    }
    oneship_lib::run()
}
