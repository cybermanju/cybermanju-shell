// CyberManju OS — Dashboard Control Commands
// Exposes web dashboard status and start/stop controls to the Tauri frontend.
//
// The dashboard instance itself is managed by Tauri (see `run()` in lib.rs),
// so these commands control the very same server the app already started
// instead of spawning a second copy that could never bind its port.

use crate::web_dashboard::WebDashboard;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Dashboard status matching the frontend DashboardStatus type.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStatus {
    pub running: bool,
    pub port: u16,
    pub url: String,
    pub active_connections: u64,
}

/// Shared dashboard state for tracking connections and lifecycle.
pub struct DashboardState {
    pub running: AtomicBool,
    pub active_connections: AtomicU64,
    pub shutdown_tx: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    pub server_thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl DashboardState {
    /// Stopped dashboard: no connections, no shutdown channel, no thread.
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            active_connections: AtomicU64::new(0),
            shutdown_tx: Mutex::new(None),
            server_thread: Mutex::new(None),
        }
    }
}

impl Default for DashboardState {
    fn default() -> Self {
        Self::new()
    }
}

fn build_status(state: &DashboardState, dashboard: &WebDashboard) -> DashboardStatus {
    let running = dashboard.running.load(Ordering::SeqCst);
    state.running.store(running, Ordering::SeqCst);
    DashboardStatus {
        running,
        port: dashboard.port,
        url: format!("http://localhost:{}", dashboard.port),
        active_connections: state.active_connections.load(Ordering::SeqCst),
    }
}

/// Get the current web dashboard status.
#[tauri::command(async)]
pub fn dashboard_status(
    state: tauri::State<'_, Arc<DashboardState>>,
    dashboard: tauri::State<'_, Arc<WebDashboard>>,
) -> Result<DashboardStatus, String> {
    Ok(build_status(&state, &dashboard))
}

/// Start the web dashboard on the configured port.
#[tauri::command(async)]
pub fn start_dashboard(
    state: tauri::State<'_, Arc<DashboardState>>,
    dashboard: tauri::State<'_, Arc<WebDashboard>>,
) -> Result<DashboardStatus, String> {
    if !dashboard.running.load(Ordering::SeqCst) {
        dashboard
            .start()
            .map_err(|e| format!("Failed to start web dashboard: {}", e))?;
        // Give the accept thread a moment to bind
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let status = build_status(&state, &dashboard);
    state.active_connections.store(0, Ordering::SeqCst);
    Ok(DashboardStatus {
        active_connections: 0,
        ..status
    })
}

/// Stop the web dashboard.
#[tauri::command(async)]
pub fn stop_dashboard(
    state: tauri::State<'_, Arc<DashboardState>>,
    dashboard: tauri::State<'_, Arc<WebDashboard>>,
) -> Result<bool, String> {
    dashboard.stop();
    state.running.store(false, Ordering::SeqCst);
    state.active_connections.store(0, Ordering::SeqCst);
    Ok(true)
}
