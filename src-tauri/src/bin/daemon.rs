use axum::{
    Router,
};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use oneship_lib::{
    db,
    mcp::create_mcp_router,
};

#[tokio::main]
async fn main() {
    println!("Starting ONE-SHIP MCP Daemon...");
    
    // An unreadable or invalid config.toml stops startup (Spec 10): settings never silently default.
    if let Err(e) = oneship_lib::config::validate_global_config() {
        eprintln!("Invalid global configuration: {}", e);
        return;
    }

    if let Err(e) = db::init_database() {
        eprintln!("Failed to initialize database: {}", e);
        return;
    }

    if let Err(e) = oneship_lib::caller_identity::close_stale_agent_instances() {
        eprintln!("Failed to close stale agent instances: {}", e);
    }

    if let Err(e) = oneship_lib::supervisor::reconcile_orphan_processes() {
        eprintln!("Failed to reconcile orphan processes: {}", e);
    }

    // The daemon owns telemetry; without its DuckDB file it cannot serve the app (Spec 25 §3.3).
    if let Err(e) = oneship_lib::duck::init_duckdb() {
        eprintln!("Failed to initialize DuckDB: {}", e);
        return;
    }

    oneship_lib::logger::log_failure("daemon", "keyring account sync", oneship_lib::kopeng::accounts::check_and_register_keyring_account());

    // The daemon is the only owner of processes and state (AGENTS.md §9.6): every background
    // loop runs here and reaches the app through `/api/events`.
    let cancel_token = CancellationToken::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<oneship_lib::events::DaemonEvent>(oneship_lib::config::event_channel_capacity_count());
    oneship_lib::supervisor::get_supervisor().set_event_tx(tx.clone());
    oneship_lib::watcher::spawn_project_watcher(tx.clone(), cancel_token.clone());
    oneship_lib::telemetry::spawn_telemetry_loop(tx.clone(), cancel_token.clone());
    oneship_lib::telemetry::spawn_continuous_profiler_loop(cancel_token.clone());
    oneship_lib::supervisor::spawn_agent_state_watcher(tx.clone(), cancel_token.clone());
    oneship_lib::kopeng::stats::spawn_quota_watcher(tx.clone(), cancel_token.clone());
    oneship_lib::kopeng::opendictate::spawn_dbus_monitor(tx.clone(), cancel_token.clone());
    oneship_lib::kopeng::rea::spawn_rea_idle_reaper_loop(cancel_token.clone());
    oneship_lib::supervisor::spawn_rearm_worker();

    let port = oneship_lib::config::mcp_daemon_port();
    let app = Router::new()
        .merge(create_mcp_router(tx, port, cancel_token.clone()));

    // Wait for SIGINT or SIGTERM to shut down gracefully and clean up child processes
    let cancel_clone = cancel_token.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("Received SIGINT (Ctrl+C). Cleaning up supervised processes...");
                }
                _ = sigterm.recv() => {
                    println!("Received SIGTERM. Cleaning up supervised processes...");
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
            println!("Received shutdown signal. Cleaning up supervised processes...");
        }

        oneship_lib::supervisor::get_supervisor().stop_all_processes().await;
        cancel_clone.cancel();
    });

    oneship_lib::logger::log_failure("daemon", "sync global agy MCP config", oneship_lib::config::sync_antigravity_global_mcp_config(port));
    if let Err(e) = oneship_lib::supervisor::write_runtime_status_snapshot(&std::collections::HashMap::new(), 0, 0) {
        oneship_lib::logger::log("WARN", "daemon", &format!("Failed to write initial runtime status snapshot: {}", e));
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|_| panic!("Failed to bind to port {}", port));
    println!("MCP Daemon listening on http://{}/mcp", addr);
    
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
        })
        .await
        .unwrap();

    oneship_lib::supervisor::remove_runtime_status_snapshot();
}
