// CyberManju OS — Main (src-tauri/src/main.rs).
// Role: Tauri desktop shell: lib, state, dashboard bridge.
// Exports: module root (see items below).
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Tauri v2 Entry Point
// Neobrutalism × Buddhist-Nepalese × Matrix × Cyberpunk

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cybermanju_os_lib::run()
}
