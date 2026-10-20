// CyberManju OS — Web Dashboard (re-export layer)
//
// The HTTP server, JWT auth and REST router all live in the shared
// `cybermanju-web` crate so the Tauri app, the Docker server and the test
// suite exercise byte-identical routing code. This module only re-exports it
// under the historical `crate::web_dashboard::*` paths.

pub use cybermanju_web::*;
