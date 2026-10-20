// CyberManju OS — Lib (crates/tests/src/lib.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: standalone (no imports).
// Contracts: signals: AGENT-.
#[cfg(test)]
mod compression;
#[cfg(test)]
mod crypto;
#[cfg(test)]
mod db;
#[cfg(test)]
mod search;
#[cfg(test)]
mod types;
#[cfg(test)]
mod web;
// <<< AGENT-4 item 9 — REST/auth suites, one module per route family >>>
#[cfg(test)]
mod agent;
#[cfg(test)]
mod api_users;
#[cfg(test)]
mod backup;
#[cfg(test)]
mod share;
#[cfg(test)]
mod sync_routes;
// <<< AGENT-4 item 10 — provider contracts against a local mock server >>>
#[cfg(test)]
mod backends_contract;
// <<< CYBERMANJU OS PUSH: one module per new brief — pre-registered so the
// agents only fill in cases, never this file. >>>
#[cfg(test)]
mod disk;
#[cfg(test)]
mod editor;
#[cfg(test)]
mod os;
#[cfg(test)]
mod repair;
// <<< ANALYSIS 100%: device identity, server todos, op-log, LAN peers,
// sneakernet bundles + the P0-6 no-row-token regression. >>>
#[cfg(test)]
mod spine;
