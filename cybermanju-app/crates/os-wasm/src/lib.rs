// CyberManju OS — Lib (crates/os-wasm/src/lib.rs).
// Role: browser volume backend (OPFS/localStorage) for Pages.
// Exports: agent, artifact, canal, compression, crypto, db, opfs_backend, os, swarm.
// Integrates: wasm_bindgen.
// Contracts: none special (pure logic; errors bubble to caller).
pub mod agent;
pub mod artifact;
pub mod canal;
pub mod compression;
pub mod crypto;
pub mod db;
pub mod opfs_backend;
pub mod os;
pub mod swarm;

use wasm_bindgen::prelude::*;

pub use compression::*;
pub use crypto::*;
pub use db::*;
pub use os::*;
pub use swarm::*;

#[wasm_bindgen(start)]
pub fn init() {
    wasm_logger::init(wasm_logger::Config::default());
    log::info!("CyberManju OS WASM module initialized");
}
