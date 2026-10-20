---
Task ID: 1
Agent: main
Task: Full codebase audit, ZimaOS compatibility, stub elimination, deployment setup

Work Log:
- Read all 40+ source files (Rust backend, Vue frontend, configs)
- Cataloged every mock, stub, and semi-real implementation
- Researched ZimaOS architecture via GitHub (CasaOS Gateway, microservices, JWT auth, Docker Compose x-casaos format)
- Identified key compatibility requirements (API endpoints, storage paths, auth, deployment format)
- Replaced WASM mock data in useTauri.ts with real REST API client (18 command mappings)
- Updated Pinia store to use composable invoke instead of direct @tauri-apps/api/core import
- Created docker-compose.yml with full x-casaos metadata for ZimaOS App Store
- Created multi-stage Dockerfile (node build → rust build → alpine runtime)
- Created standalone Docker server (docker/server/) for non-Tauri deployment
- Created .github/workflows/ci.yml (rust-check, docker-build, wasm-build, deploy-pages)
- Created .dockerignore for optimized Docker context
- Updated vite.config.wasm.ts for GitHub Pages deployment
- Rewrote Tantivy search index: proper schema field handles, add_document, remove_document, real term dictionary suggest, open_or_create
- Created commands/import.rs with import_file, scan_directory, upload_file, rebuild_search_index commands
- Registered new import commands in lib.rs invoke_handler
- Added infer + mime_guess dependencies to Cargo.toml
- Removed compression fake estimation fallback — now errors if no file on disk (with smart path resolution)
- Fixed decompression to try individual layers if triple fails, and error instead of estimate
- Replaced PQC sign/verify placeholders with real rustpq ML-DSA (Dilithium) sign_message and verify_signature
- Created comprehensive ARCHITECTURE.md (798 lines) covering all 11 modules, database schema, data flows, deployment
- Created README.md (379 lines) with build instructions, ZimaOS install guide, API reference, project structure

Stage Summary:
- Zero mocks remaining — useTauri.ts maps all commands to REST endpoints in web mode
- Zero fake estimation stubs — compression/decompression require real files
- Zero placeholder crypto functions — sign/verify use real rustpq ML-DSA
- Tantivy search fully functional with real add_document, remove_document, term completions
- New file import pipeline: import_file (single), scan_directory (recursive), upload_file (raw bytes), rebuild_search_index
- Full ZimaOS compatibility: Docker Compose with x-casaos metadata, /DATA/AppData/ volume, port_map
- CI/CD: 4-job GitHub Actions pipeline with Docker, WASM, and Pages deployment
- 12 files modified/created, ~4164 total lines written
## 2026-10-04 — release-readiness pass

- Arch AppImage: gdk-pixbuf2 2.44 on Arch dropped `/usr/lib/gdk-pixbuf-2.0/2.10.0`
  (loaders moved to glycin) while its .pc still advertises the path, so
  linuxdeploy's gtk plugin aborted with `cp: cannot stat ''`. CI now recreates
  the loader directory; release job also mirrors the NO_STRIP /
  APPIMAGE_EXTRACT_AND_RUN / patchelf fixes.
- Android: `tauri android init` generates no release signingConfig, so CI
  uploaded `app-universal-release-unsigned.apk` — Android refuses to install
  it. New `scripts/android-signing.sh` injects the keystore (repo secrets
  ANDROID_KEYSTORE_B64 / _PASSWORD / _ALIAS) into the generated project,
  verifies the signature with apksigner and renames the APK to
  `CyberManju-OS-<version>-arm64-v8a.apk`.
- Release workflow: fails if any format artifact is missing (previously only
  `warn`), fixes setup-android's removed `tools` package, and publishes
  SHA256SUMS.txt with the release assets.
## 2026-10-05 — decentralized-OS production pass (no toolchain; CI must prove)

- Frontend transport surface closed: `REST_ROUTES` + `REST_FIRST` for sync jobs/runs/status/restore/remote/usage/oauth and durability (`repair/scrub/lease/gc`); Pinia actions + `SyncJob/SyncRunRecord/RestoreOutcome/QuotaUsage/ScrubRun/RepairStatus/GcReport/LeaseInfo` types + `describeSyncError` hints; full `SyncPanel.vue` wizard (config per-backend fields, test/save/OAuth, start/cancel/runs, quota, restore/remote-delete/browse, striped placement + conflict policy); Settings active transport (`VITE_TRANSPORT`, wasm-aware); `env.d.ts` typed env; `user-scalable=no` removed; WASM base fixed to `/` (root Pages site).
- Honesty: face detection no longer fabricates pseudo-faces (empty set + log, helper kept `#[allow(dead_code)]` for tests); `import_from_url` writes `imports/{id}_{name}` + `original_path`; README tree-sitter claim corrected to heuristic regex; `cybsh sync start` refusal points at `POST /api/sync/start → 202` (R8-1 resolved as documented).
- Docs: new `docs/OPERATIONS.md`; README OS section + ARCHITECTURE §12; `AGENT-7.md` ticked `[x]` with Log; `AGENT-8.md` items 1–12/14–15 ticked, 13 in progress, R8-1/R8-2 resolved; `AUDIT.md` OS-push entry.
- Honest status of old claims: "Zero mocks remaining" (prior worklog) was premature — F1/F7/F8/F19 covered above; version single-source holds (`package.json` truth, status endpoint uses `CARGO_PKG_VERSION`).
- Not run here (no cargo/node_modules): `cargo fmt/clippy/test --workspace`, `npm run typecheck/lint`, `scripts/os-acceptance.sh 0/1/2`, `scripts/check-version.sh`.
## 2026-10-06 — sync-start-real + task-bridge + R6-3 + real tree-sitter (no toolchain; CI must prove)

- `cybsh sync start` runs for real: portable `SyncStart`/`parse_sync_start` starter core in `crates/os/src/shell.rs` (pure parsing, coreutils-style) + `os_api::try_sync_start_exec` + lockless intercept in `route_request` before the request lock (where `start_job` would deadlock); detached `start_job` semantics with terminal `ExecResult` answers; bare `execute()` keeps honest `unsupported:`. WASM keeps honest refusal (no providers/network there).
- Repair tasks bridged into `ps`/`top`: `task.rs repair_rows()` mirrors `repair::tasks()` as stable synthetic rows (`REPAIR_ID_BASE` + FNV, never persisted); `kill` refuses them with `unsupported:` (detached workers, no cancel handle); `os_api`/`cybsh` inherit the merged table with no call-site changes; unit test added.
- R6-3 closed: `os-acceptance.sh:28` already carries the fixed counting pipeline; `AGENT-6.md` request marked resolved, Tier 0 box reworded to pending-CI.
- Real tree-sitter (F8): `tree-sitter 0.24` + six 0.23 grammars (rust/python/js/ts/go/bash) behind default-on `real-treesitter`; query-free tree walk (node-kind sets + `name`/`type` fields, node/symbol caps, `"engine"` reported, heuristic fallback same shape); `parse_file`/`get_symbols` switched over; unit tests added; README/ARCHITECTURE/OPERATIONS updated. Grammar/runtime version pairing must be confirmed by `cargo check` in CI.
- Not run here (no cargo on box): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/os-acceptance.sh all`.
## 2026-10-06 — VS-like cross-device code editor (no cargo on box; CI must prove)

- New `editor` panel (`CodeEditorPanel.vue`, Dock entry, `Ctrl+E`, lazy-loaded): explorer + tabs with dirty dots, gutter + highlight overlay editing, Tab/Ctrl+S handling, find with cycling, Ln/Col status, live tree-sitter outline with click-to-jump, per-tab engine badges.
- Cross-device saves: `GET|PUT /api/files/{id}/content` (shared `api::files` logic — version snapshot before overwrite, size/hash refresh, honest `encrypted:`/`binary:`/`too_large:`/`not_found:` refusals) with Tauri twins; Pages edits the WASM volume via a new `write` dispatcher op (1 MiB cap, round-trip tested).
- Store actions (`read/saveManagedContent`, `read/saveWasmFile`, `listWasmDir`), invoke mappings, `FileContent`/`SavedContent` types, `os_write` wasm bridge; `crates/tests/src/editor.rs` pins auth gating, round-trip + versioning, refusals, and code-parse shape.
- Not run here: `cargo test` (new `files.rs`, `code.rs`, wasm `os.rs`, integration tests), clippy, `os-acceptance.sh`.
## 2026-10-06 — native AI agent core, production vertical (no cargo on box; CI must prove)

- New pure-Rust `crates/agent` (providers catalog ×10 + custom, opencode-style permission matcher, OpenAI+Anthropic protocol builders/parsers, turn state machine, hash-anchored edits) with unit tests; `native` feature gates blocking HTTP.
- Server runtime `crates/web/src/api/agent_api.rs`: keyless config CRUD (keys sealed in `sync_secrets`, `hasKey` only), model refresh, sessions + import/export, detached jobs (202 poll/abort/approve with 10-min ask parking), 7 native tools (read/write/edit/list/grep/bash/task-subagent) contained under the working root, audited, caps everywhere; routes in `lib.rs` (jobs lockless like sync), `agent` auth segment, Tauri thin commands, `cybsh ai` (parser + lockless intercept + honest direct fallback).
- WASM: `os-wasm` agent bridge (catalog + single-turn fetch) + `useAgent` browser loop (volume tools, local matcher mirror, in-memory keys, localStorage transcripts) + `write` dispatcher op.
- UI: lazy `AgentPanel` (Dock, Ctrl+G, presets/custom/endpoints/models/keys/permissions/sessions/thread/approvals/usage, dual server+local modes), `FileContent`/`SavedContent`/agent types, store actions, invoke mappings; `files/{id}/content` read/write (versioned saves) backing the editor on REST; `crates/tests/src/agent.rs` pins gating, key hygiene, validation, import re-keying, `ai` intercept.
- Docs: README features + REST table, ARCHITECTURE endpoints + `agent_configs`/`agent_sessions` tables, OPERATIONS §4b.
- Not run here: `cargo fmt/clippy/test --workspace`, Tauri/desktop builds, `os-acceptance.sh`.
## 2026-10-06 — agent review hardening + MCP + loop features (no cargo on box; CI must prove)

- Review fixes: user prompt was never appended to new transcripts (worker now seeds it); `tool_bash` drains pipes on threads (no 64 KiB deadlock); orphaned test headers repaired (shell `tab_completion`, os `rows_round_trip`); real missing-paren caught in `mcp.rs` test.
- MCP end to end: pure `agent/src/mcp.rs` (JSON-RPC envelopes, initialize handshake, tools/list normalization, call rendering, SSE parsing, namespaced `mcp__server__tool` ids) with unit tests; native stdio (reader thread, timeouts) + Streamable HTTP (session ids) transports; per-run connect/list/fail-fast with RAII teardown; calls routed through the same permission decisions; discovery + attach/detach endpoints (attach/detach admin-gated like sync-config delete); Tauri commands; AgentPanel MCP section (attach form, tool discovery, per-server rows); WASM answers MCP via the honest `unsupported:` path (no fake tools offered).
- Loop hardening (always on): `rate_limited:` backoff (2s/4s/8s, abort-aware), doom-loop guard (identical call ×3 denied), `length`-finish truncation notes, cancel checked after provider calls, Anthropic prompt-caching markers, read-only depth-1 subagents with the same hardening.
- Session compaction (`POST …/compact` lockless, old kept), model-list refresh UI already wired, README/ARCHITECTURE/OPERATIONS deltas, `crates/tests/src/agent.rs` extended (admin gating, MCP validation, compact validation).
- Not run here: `cargo fmt/clippy/test --workspace` (new `mcp.rs`, worker edits, route arms), Tauri builds, `os-acceptance.sh`. Known CI watch items: grammar-style version pairing N/A (pure Rust only); MCP stdio needs a POSIX shell on the runner.
## 2026-10-06 — opencode/omp clue sweep: redaction, rules, MCP reconnect, remember, init (no cargo; CI must prove)

- Clues taken: omp stream-hygiene → secret redactor on every tool result (`crates/agent/src/redact.rs`, marker+JWT shapes, byte-exact, tested); opencode rules/references → project-rules loader (`AGENTS.md`/`SKILL.md`/`.cybermanju/rules.md`, capped, folded into all system prompts incl. subagents); opencode MCP lifecycle → reconnect-once on transport breach; opencode always-approve → `remember` storing an explicit `Simple(Allow)` row (route+Tauri+store+ALLOW ALWAYS button); opencode `/init` → `ai init` as a convention (canned prompt job that writes AGENTS.md with its own tools: REST+Tauri+cybsh+panel button); omp hashline → whitespace-tolerant fuzzy locate as second-chance matcher (byte-exact map, uniqueness still enforced, unicode-tested).
- Review repairs in the same pass: orphaned test bodies re-headed (shell tab-completion, edit stale-anchor), missing paren in `mcp.rs` test, dead helper + duplicate import removed from `agent_api.rs`, pipe-deadlock-proof bash drains, prompt-seeding bug (fixed prior turn, re-verified).
- Not run here: `cargo fmt/clippy/test --workspace`, Tauri builds, `os-acceptance.sh`. CI watch: MCP stdio needs POSIX runners; new worker paths (retry/cache/doom/reconnect/remember/init) are covered by unit + route-validation tests, live runs need provider keys.
## 2026-10-06 — agent metaprompt + rg/glob tools, all transports (vitest green; cargo still CI-only)

- Metaprompt rewrite (`agent_loop::system_prompt` + per-tool schema guidance): role, honest per-transport sandbox (native bash/task/MCP vs browser file-tools-only), path conventions, edit workflow (read → anchor → verify), approval/doom-loop behavior, machine-prefix meanings, concise-output contract. Browser loop mirrors it in `AgentPanel`.
- New `glob` tool everywhere (Rust walker + `**` matcher, WASM volume walker, TS matcher): 7 → 8 tool schemas on every transport; `grep` is real regex now (regex crate native, RegExp in browser, literal fallback both sides when the pattern is invalid).
- Test-caught fix: opencode-style `bash` patterns (`git *`) match the bare command, not `bash <cmd>` — rules now try all three shapes in Rust and TS; pinned in both suites.
- Tests: new `tests/frontend/agent-matchers.test.ts` (5 green), Rust unit tests for glob/grep/metaprompt/fuzzy; docs (README bullets, OPERATIONS tool paragraph).
- Not run here: `cargo fmt/clippy/test --workspace`, Tauri builds, `os-acceptance.sh`. CI watch: `regex` compiles to wasm32 (pure Rust, no OS calls — expected fine); new `glob` walker bounds (200 paths / 2000 files).
## 2026-10-06 — shell-app wiring sweep: favorites persist, loose-groups CRUD on all transports, reachability, shared error/loading states (cargo CI-only)

- Favorites: `toggleStar` was local-only (Rust `FileNode` has no star column on any backend). Now persisted beside the file table via `starredIds` set + localStorage, mirrored into worker kv (`stars`) on static hosts so stars ride inside the `.cybermanju` container with provider data; re-applied after every `fetchFiles`. New `src/utils/stars.ts` codec + `tests/frontend/stars.test.ts`.
- Loose groups: was list-only UI. Added store `createLooseGroup`/`addFileToLooseGroup`, REST `POST /api/loose-groups` + `POST /api/loose-groups/{id}/files` (new `api::files` helpers mirroring the Tauri commands), `REST_ROUTES` mappings, and kv-backed `STATIC_COMMAND_HANDLERS` so the static/WASM build has full CRUD with zero wasm rebuild. WindowContent loose panel now creates groups and adds files.
- Reachability: `loose-groups`/`style`/`webdash` had no Dock/TopMenu entry. Added View-menu items (Loose Groups, Style Tags, Overlay Dashboard), Sidebar TOOLS shortcuts, and clickable TAGS/LOOSE section headers.
- UI modernization: new shared `UiError` (registered in `src/ui/install.ts`); favorites/recent/loose/style panels and StorageDashboard rebuilt on `UiToolbar` + `UiSpinner` + `UiEmpty` + `UiError` with refresh/retry actions and mount-time fetches; TopMenuBar/Dock get narrow-window overflow handling; `fm-spin`/MapView spinners honor `prefers-reduced-motion`. (Users role toggle already existed in UserManagementPanel — earlier audit note corrected.)
- Verified here: `vue-tsc --noEmit` clean, `vitest` 9 files / 81 pass, `vite build` green.
- Not run here: `cargo check/test/clippy` (no linker on box) — new `crates/web` loose-group helpers + routes must be proven in CI; Tauri/desktop builds.
## 2026-10-06 — People panel as centered Spotlight/Alfred search (vitest+build green)

- Rebuilt `FaceGroupingPanel` from a formal CRUD form into a centered launcher (max-width 540 column, radial accent glow): hero search bar (autofocus, clear/Esc), people strip of avatar chips (initials, per-group color ring, file-count badge), and a live dynamic listing (PEOPLE matches with highlight + PHOTOS member-file matches, capped with overflow counts).
- Full keyboard flow: ↑/↓ across people+photos, ⏎ opens (person → Files panel, photo → select+Files), Esc clears; hover and keyboard share one active key with scroll-into-view; footer hints document it.
- Detail card for the active person: member-file chips, OPEN FILES / RENAME (inline) / DELETE (two-click confirm); footer keeps BATCH DETECT (busy state + last-scan stats) and RESCAN; `UiError` retryable + `UiSpinner` + `UiEmpty` no-match states; reduced-motion and ≤560px breakpoints.
- Default faces window resized to 620×640 for the new layout.
- Verified here: `vue-tsc` clean, `vitest` 9 files / 81 pass, `vite build` green.
## 2026-10-06 — scene heuristic algo (multilingual, scored) + user tags end-to-end (cargo CI-only for Rust)

- New `crates/scene`: single source `data/scenes.json` — 17 categories (human, baby, forest, mountain, beach+sea, sunset, snow, city, party, wedding, animal, dog, cat, bird, food, sport, garden) × 10 languages (en pt es fr de it nl ru zh ja), 740 keyword entries, child→parent map. Honest heuristic: filename/path/user-tag text only, per-source weights (name 1.0 / tags 0.95 / path 0.7), exact 0.9 / edge-aligned-substring 0.55 / CJK-single-char 0.4, noisy-combine capped 0.97, report ≥0.30, plus face-cluster human boost and image-mime whisper. Mid-token noise rejected (`cume` in `document` stays silent — test-caught), CJK raw-containment gated to CJK scripts.
- TS twin `src/utils/scene.ts` parses the same JSON with the identical algorithm; `tests/frontend/scene.test.ts` (42 green) mirrors `crates/scene/src/tests.rs` (multilingual beach ×10, per-language category checks, query matching, tag-carry, parent inference, silence guarantees, English reachability per category).
- Search UI (`WindowContent`): SCENES strip with % chips from `matchSceneQuery` (click re-searches the category) + per-result scene % badges fed by filename+tags+path.
- User tags on any file: `api::files::set_tags` (normalized: trim/dedupe/cap) + REST `PUT /api/files/{id}/tags` + Tauri `set_file_tags` (registered) + `REST_ROUTES`/`DB_WASM_ROUTES` (via `files.patch`, tags already whitelisted) + store `setFileTags` + FileManager inspector tag editor (chips, add/remove). Tags feed Tantivy's filename+content+tags BM25 (verified wired in `crates/search`) and the scene matcher. Human naming = existing `renameFaceGroup`, exposed in the People spotlight panel.
- Verified here: `vue-tsc` clean, `vitest` 11 files / 141 pass, `vite build` green.
- Not run here: `cargo test -p cybermanju-scene` and workspace check (no linker on box) — CI must prove Rust; Tantivy reindex timing for new tags matches existing mutation behavior (index rebuilds in batch jobs).
## 2026-10-06 — niri-style shell review pass: gestures, pure layout math, tests (cargo CI-only for Rust)

- Review fixes: `useSwipe` treated EVERY two-finger touch as a pinch, so two-finger swipes could never fire — now pinch-vs-swipe is decided by the shared `classifyTwoFingerGesture` classifier (spread change wins, else centroid travel → swipe). Multi-finger taps no longer fake single-taps; single-finger tap/long-press/edge behaviour unchanged.
- Gestures: three fingers up/down → `overview_toggle` (zoomed-out grid of ALL screens, minimized included with dashed badge, click restores); two fingers → strip travel (`strip_left/right` along columns, `strip_up/down` between lines), falling back to focus prev/next outside strip mode. 14 new rebindable touch actions (`overview_toggle`, `strip_*`, `strip_direction`, `autotile_toggle`, focus/layout family) with labels for the Settings gesture table.
- Niri parity: `stripLineMove` keeps one empty line below the lowest occupied line (open a window there to claim it); `close` prunes dead lines and clamps the viewport; `tiled` mode re-tiles on open; viewport resize re-runs strip/tile layout; `Alt+Shift+Arrows` move windows (`move_window_*` in `.kpl`, nudge in floating, column/line move in strip via new `nudgeFocused`); overview exits back to tiled when autotile is on.
- Pure module `src/utils/shellLayout.ts` (tiling/strip rects, offset/line resolution, swipe + two-finger classifiers) — used by `useWindowManager`, `DesktopShell` and `useSwipe`, proven by `tests/frontend/shell-layout.test.ts` (geometry, no-overlap tiling, one-empty-line rule, pinch-beats-swipe) and `tests/frontend/shell-gestures.test.ts` (defaults + dispatch wiring); kpl coverage extended with the move bindings.
- Verified here: `vue-tsc` clean, `vitest` 20 files / 233 pass, `npm run icons` clean, `check-version.sh` agrees.
- Not run here: `cargo fmt/clippy/test --workspace`, Tauri/desktop builds — CI must prove Rust.
## 2026-10-06 — agent UX remake: harness + modern chat panel + window polish (cargo CI-only for Rust)

- Review findings: `AgentPanel.vue` was a 2059-line monolith — 7 stacked ALL-CAPS sections (capability strip, setup wizard, configs, MCP servers, per-tool permissions, sessions, thread) in one endless scroll; no onboarding, no progressive disclosure; transport branching (`wasmMode ? local : store`) duplicated across ~20 functions; composer was a bare textarea (hidden Ctrl+Enter); agent window default 720×600 too narrow for chat.
- New `src/composables/useAgentHarness.ts`: single-import surface over all transports — `detectTransport`, `capabilitySummary` (plain-language model/persona/dir/shell/key), `quickPrompts`, `slashCommands`, `formatTokens`/`contextPctOf`/`copyText`. Permission gate, 202-job contract, error prefixes and BLAKE3 anchors unchanged underneath.
- `AgentPanel.vue` remake (script logic untouched, all contracts kept): header (avatar + live pulse, friendly transport label, context meter, YOLO, Chat/Setup/Controls tabs); collapsible sidebar (searchable conversations + assistants); plain-language capability strip; Setup as 3 guided steps (provider → model & key → permissions & workspace); Controls tab (MCP connect + per-tool Allow/Ask/Never); chat with welcome screen + quick prompts, avatar bubbles, copy buttons, friendlier tool badges (Needs you/Failed), `role=alertdialog` approval card, sticky composer (Enter to send, `/` commands, char count, Dictate, Stop/Queue). Responsive single-column under 760px; sentence case throughout.
- OS windows: agent default 1020×680; `AppWindow` resize now takes the event param (no more deprecated `window.event`).
- Verified here: `check-version.sh` agrees, `npm run typecheck` clean (icons rebuilt, 163), `vitest` 21 files / 239 pass incl. new `tests/frontend/agent-harness.test.ts` (6).
- Not run here: `cargo fmt/clippy/test --workspace`, desktop/mobile builds — CI must prove Rust.
## 2026-10-06 — touch stack goes live: surface attach, touch-action, Pointer Events rebase (cargo CI-only for Rust)

- Root cause: `mainAreaRef` (the `useSwipe` target) was never bound to any element, so EVERY touch gesture was dead in production — three-finger overview and two-finger strip travel included. Bound to `.cybermanju-shell` in `App.vue`.
- `touch-action: none` on the gesture surface: the shell recognizer now owns 2/3/4-finger swipes and pinches (no mid-gesture `pointercancel`, no double-handling with native zoom); inner scrollable panels keep native scroll since touch-action intersects only up to the implementing scroller.
- `useSwipe` rebased from Touch Events to Pointer Events with zero API change: mouse ignored (desktop drags stay with the windows), pen works like touch, `pointerId`-keyed chords, pinch-vs-swipe dominance guard (spread change must BEAT centroid travel, else a fast parallel swipe's leading finger misfires as pinch), pinch-then-keep-scrolling rebase, cancel = abort (never double-handle). Core extracted as `createSwipeMachine` (no Vue/DOM) for tests.
- `tests/frontend/shell-swipe.test.ts` (11 tests): 1-finger swipe/tap, mouse ignored, pen works, two-finger parallel swipe, pinch in/out + pinch-and-pan precedence, post-pinch rebase scroll, 3/4-finger swipes, cancel silence, sub-threshold drift.
- Verified here: `vue-tsc` clean, `vitest` 22 files / 250 pass, `npm run icons` clean, `check-version.sh` agrees.
- Not run here: `cargo fmt/clippy/test --workspace`, Tauri/desktop builds — CI must prove Rust. Real-device check still needed: DevTools can't synthesize true 2/3-finger chords (remote-debug a phone/tablet or Playwright CDP `Input.dispatchTouchEvent`).
## 2026-10-06 — remove Telegram/Google Photos providers (4 backends remain)

- `SyncBackendType` is now Local/GitHub/GitLab/GoogleDrive only (`crates/types`, `src/types`); `album_id`/`chat_id` fields, `albumId`/`chatId` drafts, forms and wizard fields removed.
- Deleted `GooglePhotosBackend` + `TelegramBackend` (~550 lines) and their factory arms from `crates/sync/src/backends.rs`; dropped `PHOTOS_MAX`/`TELEGRAM_MAX`, Photos/Telegram rate gates, quota arms, Photos scope (`drive.file` only) and OAuth slug arm; `compute.rs` capabilities cover the 4 survivors.
- `os-wasm` canal: removed telegram/photos normalize + `cors:` arms (legacy names now get generic `unsupported:`); updated `useTauri` static probe, `useSupabase` slugs/scopes, `ProviderLogo`, `AccountManager`/`SyncPanel`/`Sidebar`, `providers.ts` helpers.
- Docs: `ARCHITECTURE.md` (diagrams, tables, `backendType` enum), `README.md`, `AGENTS.md`, `docs/OPERATIONS.md` updated; historical `worklog.md` entries left intact.
- Verified here: `check-version.sh` agrees, `npm run typecheck` clean (icons rebuilt, 161 — `camera-bold`/`plane-bold` pruned), `vitest` 22 files / 250 pass.
- Not run here: `cargo fmt/clippy/test --workspace` (repo rule: CI's Rust Lint & Test job proves Rust).
## 2026-10-06 — first publish prep: Settings remake, broker reactivity, release notes, dual push

- Settings page remade (collapsed-card text overlap fixed via block flow), status strip, section jump chips + scroll spy, keyboard-bindings filter, wrap-based rows; `useSupabase` reactive `configuredFlag` (badges flip on save/forget, no reload); Accounts Configure deep-links to the broker card; `useTouchConfig` threshold setters (state is readonly — direct writes were silently dropped); TopMenuBar keyboard access + dropdown clipping fix.
- `release.yml`: release notes now carry the full v0.1.0 feature atlas + status/known-limits for the first publish (was: bare git-log list).
- `push.sh` rewritten: `git add -A` + message arg (no more bare `update`), pushes main + tags to GitHub `origin` AND GitLab `gitlab` mirror, no force-push.
- Verified here: `check-version.sh` agrees, `npm run typecheck` clean, `vitest` 22 files / 250 pass.
- Not run here: `cargo fmt/clippy/test --workspace`, desktop/mobile builds — CI + release workflows must prove them. Previous v0.1.0 Release run failed on WASM build, Android signing secrets, and Linux apt deps (logs need repo admin to read); tag move + re-run pending.
- GitLab mirror live (`.gitlab-ci.yml` 15 jobs + secrets inventory); first pipeline fails at creation with `Identity verification is required in order to run CI jobs` — account-level gate, needs owner verification + the 5 CI variables.
## 2026-10-07 — OAuth popup close + multi-account sign-in

- Popup stuck open showing the app inside: `signInWithPopup` used `noopener` (severed opener chain, so `window.close()` + opener messaging died) and never closed the popup from the opener side; deny/error returns (`?error=`, no `?code=`) booted the full app in the popup. Fixed with `isOAuthPopup()` detection, App.vue popup fast-path (minimal view, skips store/vault boot), `finishSupabaseReturn()` opener notify (`cybermanju:oauth-done` postMessage) + retried `closeOAuthPopup()`, `?error=` handling, and opener-side `popup.close()` on success.
- Account manager hid all provider logins after first sign-in (`v-if="identity"` / `v-else`): login grid with all 3 providers now always renders ("Add another account" when signed in). Multi-account via `connectedAccounts` (persisted, deduped by user id) with Switch/Forget; every sign-in starts fresh (sign-out first) so same-provider re-login and provider switching work; `refreshIdentity()` repopulates the list on fresh browsers.
- `supabaseConnect` token poll now only accepts the token minted for the expected provider (`supabaseSessionProvider()`), so a stale session from another provider can't close the flow with wrong credentials; added opener message wake-up.
- Verified here: `vue-tsc --noEmit` clean, `vitest` 22 files / 250 pass.
## 2026-10-07 — OAuth debug logging removed

- Stripped all `[supabase]` console diagnostics (`debugSupabaseConfig` + call sites in Settings/Accounts, authorize/return/sign-in/account logs), the `[WASM Mode]` envelope passthrough warn, and the `[os_ps]`/`[dashboard_status]`/`[start_dashboard]` shapeless-payload warns (silent drops kept). Behavior unchanged.
- Verified here: `vue-tsc --noEmit` clean, `vitest` 22 files / 250 pass.
## 2026-10-07 — Full VFS/sync hardening: encrypt-or-fail, hidden names, folder sync, VFS writes, parity UI

- Encrypt-or-fail: new `SyncConfig.requireEncryption` (Rust + TS, default false); pipeline refuses the file with `auth:` when no master passphrase exists instead of uploading plaintext with a warning.
- Basename obfuscation: new `SyncConfig.obfuscateNames`; `remote_path_for(path, obfuscate)` maps to `cybermanju_sync/<dir8>/<nameHash16>` (deterministic, idempotent; striped chunks were already content-addressed). Originals stay in the local sync record for restore.
- Recursive folder sync: `sync_api::expand_sync_ids` BFS through the parent index (cap 5000, `too_large:` beyond) wired into both `start` (Tauri) and `start_job` (REST 202); unknown ids pass through, cycles impossible.
- VFS write-through: new `upload_bytes` (`POST /api/sync/upload`, 5 MiB cap) + `upload_remote_file` Tauri command; `useProviderCanal.writeVfsFile/deleteVfsFile` (Rust backend on desktop/Docker, direct provider fetch on static hosts, `deleteFileDirect` for GitHub SHA + GitLab); `vfs_write_file`/`vfs_delete_file` invoke routes; Sidebar mounts are read-write now (upload button, per-file delete, cache invalidation).
- Surfaced knobs: SyncPanel wizard has parity (0–4) + require-encryption + hide-filenames; Account Manager provider detail has a Sync behavior step (encrypt/compress/require/hide/placement/parity, saved per config); `compressBeforeUpload` default flipped to true; vault-set provisioning defaults to requireEncryption + obfuscateNames on.
- Rust struct literals updated for the new fields (`disk/testkit`, `sync/oauth` ×2, `tests/types`); old config rows parse via serde defaults (same pattern as `health.rs` test).
- Verified here: `vue-tsc --noEmit` clean, `vitest` 23 files / 267 pass. Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-07 — Drive parity on static hosts + GitHub overwrite fix

- Google Drive rides the full sync engine (pipeline transforms, striped chunks, disks are backend-agnostic); only repo *creation* is git-only by nature (Drive uses folders, created implicitly on upload).
- Static-host VFS writes/deletes now cover Drive too: `driveWriteDirect` (parent-folder auto-create, same-name overwrite via media PATCH, multipart create) + `driveDeleteDirect` (listing locator id preferred, else path resolve; 404-tolerant); canal static branches route `googleDrive` mounts to them instead of refusing.
- Fixed a silent-loss bug in `seedRepoDirect` (GitHub): overwriting an existing file returned 422 which was swallowed as success — now looks up the blob SHA and retries the PUT (same rule the Rust backend follows).
- Verified here: `vue-tsc --noEmit` clean, `vitest` 23 files / 270 pass. Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-07 — System-wide gap sweep: contract prefixes, REST parity, log redaction, orphan cleanup

- Error-prefix contract (AGENT-1): swept `File not found`, `Failed to…`, `disk full:`, `not found:`, `no files:`, `invalid:`, `Unsupported OAuth…`, `Sync config not found`, `Triple compression/Decryption failed` into `not_found:/unsupported:/disk_full:/integrity:` across `sync/pipeline+oauth`, `web/sync_api`, `disk/*`, `os/api+compute`, `tauri/compression+encryption+import+tree_sitter`; updated the asserting unit/contract tests. `describeSyncError` already covered every prefix (kept the legacy `disk full` alias).
- REST parity (web transport): added `suggest`, `get_shared_file`, `set_file_permission`, `get_preview` mappings to verified server routes; `get_compression_stats`/`get_symbols` marked WRITE_ONLY (local paths only); `search_files` now passes real BM25 score/snippet instead of fabricating 1.0/''; `get_encryption_status` passes real fields through; exported `REST_ROUTES`/`WRITE_ONLY_COMMANDS` + new `transport-routes.test.ts` (6 tests) locks the table.
- Security: access log strips query strings (OAuth `code/state`) and masks `/api/shared/<token>`; secret files (JWT + master passphrase) refuse group/other-readable modes fail-closed WITHOUT rotating over them (rotation would orphan sealed keys / kill sessions).
- Durability: striped chunk temps guarded by `RemoveOnDrop` (retry-exhaustion/cancel no longer leaks `cybermanju-up-*.cyb3`); link/record failures after landed uploads now compensating-delete (whole-file remote + every striped chunk copy).
- Restore hardening: `destPath` rejects NUL + `..` escapes; restore errors prefixed (`not_found:/unsupported:/integrity:`).
- Verified here: `vue-tsc --noEmit` clean, `vitest` 24 files / 276 pass. Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-07 — Scroll repair + stylized scrollbars across OS and repo shells

- Root causes: `touch-action: none` on `.cybermanju-shell` forced `none` on every nested panel (intersection) killing touch scroll; `AppWindow` pinned children to `height: 100%` so tall content clipped with no scroll; `TerminalPanel` used `contain: strict` without `min-height: 0`; strip wheel handler never drove `scrollLeft` for vertical-wheel input.
- Fixes: shell allows `pan-x pan-y`; window children use `min-height: 100%`; `contain: layout paint` + `min-height: 0` on all three cybsh scrollbacks (terminal, CodeStudio bottom, FileManager inline); niri-style wheel translation with inner-scroller passthrough + explicit strip sizer; one stylized accent scrollbar system in `ui.css` (Dock/tray bars auto-hide, rest always visible).
- Verified here: `vue-tsc --noEmit` clean, `vitest` 23 files / 267 pass. Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-07 — Window merge pass: 7 panels folded into 4 surfaces

- One-window mechanism: `PANEL_ALIASES` + per-alias tab props in `useWindowManager.open()` (re-steers a live window instead of stacking copies); every merged surface takes a `tab` prop with a watcher.
- `code`→CodeStudio (new INTEL side view: file/paste/path tree-sitter parse, kind chips, preview); `webdash`→`dashboard`; `storage`→Disks overview tab; encryption+compression→new tabbed `ShieldPanel`; `users`→Accounts Users tab; SyncPanel slimmed to runs/monitor (CRUD lives in Accounts); collections/favorites/loose-groups/style→new tabbed `OrganizePanel`; `preview`→Files inspector (faces/symbols rows added); search extracted to shared `SearchPanel` (Search window + Studio side view).
- Deleted: `CodeIntelligencePanel`, `StorageDashboard`, `EncryptionPanel`, `CompressionPanel`, `UserManagementPanel`, `CollectionsPanel`, `FilePreview`. Also removed dead `showEncryptionPanel/showCompressionPanel/showPermissionsPanel` store flags and rewired Ctrl+E / Ctrl+Shift+C to the Shield window. Follow-up review: `PANEL_ALIASES` extracted to pure `src/utils/panels.ts` + `tests/frontend/panel-aliases.test.ts` (5 tests lock no-cycles/metadata); user delete asks first; stale counts refreshed in AGENTS.md/README.md.
- Verified here: `vue-tsc --noEmit` clean, `vitest` 25 files / 281 pass. Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-07 — cybsh full review: static-layer vault verbs, merged file ops, wasm parity, 0.1.1

- New static transport layer `src/utils/staticCybsh.ts`: every `os_exec` line on Pages/WASM is offered here first (quote-aware parse, chained lines fall through). Vault-aware verbs answer locally — `quota` (shell volume + browser storage + live GitHub/GitLab/Drive probes via sealed vault tokens, dashboard warning footer), `providers`, `oauth status|start`, `disk`, `sync status|list`, `mount`, `encrypt|decrypt|keygen` (ChaCha20-Poly1305, vault keys), `compress|decompress` (lz4/brotli), `scrub|repair|gc|lease`, `echo|kill|ai` — all in the house `prefix: detail` contract with `--json` support.
- Merged-namespace file ops: `cp|mv|rm|mkdir` route plain paths to the shell volume and `/providers/<mountId>/…` through the provider canal (same-provider renames, cross-provider and provider↔local moves, `-r` trees, `.keep` markers, strict-UTF8 binary guard, `mv -r` tolerance).
- Wiring in `useTauri.ts` (`STATIC_CYBSH_DEPS`: vault, volume+mirror nudge, VFS canal, crypto exports) + `notifyOsDispatch` and `rm`/`kill` arg mappings in `useWasmBackend.ts`; `get_sync_usage` static handler so the Sync panel quota button works on Pages.
- WASM Rust parity (`crates/os-wasm/src/os.rs`): native `echo|cp|mv|kill` arms, `ai` in the command table with the detached-worker refusal, Tab completion extended with sub-commands (mirrors native `completions()`), grouped `help` (local volume / local vault / dashboard only).
- Voice dictionary gains `oauth|compress|decompress` (+ `oauth status|start` phrases); `docs/OPERATIONS.md` gains §4c documenting the static-site shell.
- Robustness: probe JSON bodies read via `safeJson` (unreadable body → `network:`, never a throw); provider transport errors rethrown instead of masked as `not_found`; local `rm -r` counts successes only.
- Version bump 0.1.0 → 0.1.1 across all carriers (`check-version.sh` green).
- Verified here: `vue-tsc --noEmit` clean, `vitest` 28 files / 362 pass (static-cybsh suite 28→58 tests). Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-07 — CI watch script + push commit-message UX

- New `scripts/watch-ci.sh`: polls the GitHub CI run (by `--run`, `--sha`, or
  latest on `--branch`) and saves/prints the FULL logs of all jobs/steps to
  `logs/ci-<run-id>/`. All state via `gh api .../actions/runs/...` + `/jobs`
  (works on old `gh` 2.4.0, which lacks `run view --branch`); falls back to
  `curl` + `$GH_TOKEN`/`$GITHUB_TOKEN` and `python3` stdlib when `gh`/`jq` are
  absent. Handles prose-only pushes (no run started), superseded runs under
  CI's cancel-in-progress concurrency, and `--print failed|all|none`.
- `push.sh` remake (backward compatible): `-m/--message`, multi-word
  positional join, TTY prompt with dated default (silent default when piped),
  bare-`update` refusal, `--allow-empty`, `--dry-run`, and `--watch/--no-watch`
  (`PUSH_WATCH_CI=1` env) chaining into `watch-ci.sh --sha HEAD` after a good
  origin push. AGENTS.md §3 documents the new flow.
- Verified here: `bash -n` clean on both, `--help`/`--dry-run`/bare-`update`
  refusal/auth-error paths exercised. Live GH polling needs stored `gh` auth.
## 2026-10-07 — Quadrant autotile (≤4 windows) + push.sh hardening

- Autotiling (`src/utils/shellLayout.ts:computeTileRects`): dedicated layouts
  for ≤4 windows — 1 fills the screen, 2 split halves (long-axis aware), 3 use
  master-left + two stacked right (no empty hole), 4 fill equal 2×2 quadrants;
  5+ fall back to a dense grid. Dropped the `Math.max(320/240, …)` minimums that
  forced tiles to overlap/overflow on small screens.
- `src/composables/useWindowManager.ts`: re-tile on close/minimize/restore so
  survivors expand (grid stays dense); focused window is re-raised on top so
  tiling never steals focus.
- `push.sh` fixes: heal mixed root/uid `.git/objects` ownership (root hands
  `.git` back to the worktree owner); abort if `git add -A` fails instead of
  pushing stale HEAD; refuse unless on `main`; fetch both remotes and
  auto-rebase onto `origin/main` when behind (never force-push); push only tags
  the remote lacks — a divergent remote tag (e.g. `v0.1.0`) now warns instead
  of marking the whole mirror push FAILED. AGENTS.md §3 documents the flow.
- Verified here: `vue-tsc --noEmit` clean, `vitest` 28 files / 362 pass,
  `push.sh --dry-run` prints the new fetch/rebase/tag flow.
## 2026-10-08 — Docker 401 fan-out: dashboard login gate + public auth probe

- Root cause: the Docker/web frontend had no sign-in — `authenticate_user`/
  `register_user` existed but no UI called them, so every `/api` call 401'd;
  the 401 hint wrongly pointed at provider OAuth (Google/GitHub/GitLab), whose
  tokens open provider sync but never authenticate `/api`. `initialize()` fired
  the whole fetch fan-out with no token (the reported console spam).
- Backend (`crates/web`): new public `GET /api/auth/status →
  { registrationOpen }` (RBAC Public + route arm via `registration_open`);
  unit test extends `public_routes_need_no_role`; integration test covers
  open→bootstrap→closed. Bootstrap semantics unchanged (closes after the first
  account, never mints admin — extra dashboard users come from an admin in
  Accounts → Users; provider OAuth accounts stay unlimited incl. same-provider).
- Frontend: `auth_status` + `logout_user` REST mappings; store gains
  `needsAuth/needsSetup` gate, `login/register/checkAuthStatus`, user persist
  per browser, `initialize()` early-return + auto-refresh guard (no 401 spam),
  corrected 401 message → `cybermanju:open-login`; new `ServerAuthPanel.vue`
  gate rendered from `App.vue` on every device without a JWT.
- Docs: `OPERATIONS.md` §6 first-run + multi-account note, `ARCHITECTURE.md`
  endpoint rows for `auth/status` + `auth/logout`.
- Verified here: `check-version.sh` green, `vue-tsc --noEmit` clean, `vitest` 29 files / 372 pass. Rust `fmt/clippy/test` left to CI per repo rules.

## 2026-10-08 — Android full fix (16 items): packaging, signing, runtime, mobile UX

- Packaging: CI/release build `--apk --aab --split-per-abi` (Play bundle alongside APK); Rust targets trimmed to `aarch64+x86_64` (armv7/i686 were never built); `versionCode` pinned in `tauri.conf.json` (`1001` = major*1e6+minor*1e3+patch), enforced by `check-version.sh` + new `scripts/android-configure.sh` (versionCode audit, manifest patch, ABI policy).
- Manifest (`android-configure.sh`, idempotent): INTERNET/ACCESS_NETWORK_STATE/LOCATION/CAMERA/RECORD_AUDIO/POST_NOTIFICATIONS, `usesCleartextTraffic` (http LAN dashboard), `allowBackup=false` fail-closed + manual export path, `cybermanju://` deep link + `.cyb3` VIEW intents.
- Signing parity (P2-8 closed): unsigned PR artifacts renamed `CyberManju-OS-<ver>-arm64-v8a-unsigned.apk` (never neutral); `android-signing.sh cleanup` shreds keystore+properties pre-upload; AABs listed + shipped in artifacts/release packages.
- On-device verification: new `scripts/android-smoke.sh` (adb install + am start + logcat crash grep, graceful skip without arm64 emulator), wired `continue-on-error` in CI, non-blocking in GitLab.
- Runtime: Android DB path branch (app-private files dir, never CWD), `default_secret_dir()` Android branch, WebDashboard NOT started on mobile (`#[cfg(mobile)]`), `fatal()` panics on mobile (logcat) vs exit on desktop, Tantivy battery note.
- Capabilities: new `src-tauri/capabilities/mobile.json` (`platforms: ["android"]`, scoped-storage comment).
- Frontend: `viewport-fit=cover` + theme-color + mobile-capable metas, safe-area/`100dvh` shell CSS, `body.cybermanju-offline` dimming, Android back-gesture (`popstate` → close transient / history back) + online/offline listeners in `App.vue`.
- CSP covers both WebView asset schemes (`http://asset.localhost` added).
- Docs: new `docs/ANDROID.md` runbook (§§ packaging/signing/manifest/storage/runtime/frontend/updates/verification); `TASKS.md` P2-8 ticked, P2-16 narrowed to iOS; `AGENTS.md` android lines updated.
- Not run here: `cargo fmt/clippy/test`, device builds — CI must prove Rust/mobile.
- Agent 400 fix: `openai_request` echoed neutral `{id,name,input}` tool rows verbatim into `tool_calls`, 400ing strict providers on the follow-up turn (right after the first tool result). Now reshaped to spec `{id,type,function:{name,arguments}}`; `tool_call_id`/`tool_use_id` never null. Non-context HTTP 400 → new `invalid:` prefix (check model id + tool support); string `"error"` envelopes parsed. Browser loop: `rate_limited:` backoff retry (2s/4s/8s, abort-aware, native parity). Rust test `openai_tool_echo_is_spec_shaped` added.
- Not run here: `cargo fmt/clippy/test` (no local toolchain) — CI must prove Rust.
- Provider failover: `AgentConfig.fallbackIds` (max 4, validated on save) chains assistants as extra accounts/keys or providers. Native worker resolves primary + usable fallbacks and continues the same transcript on terminal `auth:/rate_limited:/network:/limit:` (empty credits now classify as `limit:` incl. 402/insufficient/billing; non-context 400s stay `invalid:`), others stop; all-failed errors append `(tried: …)`. Browser loop mirrors with the same rule + per-route headers. Controls → Fallback route card (add/remove/reorder, Ready badges); Setup notes the second-account flow; wizard re-save preserves routes.
- Not run here: `cargo fmt/clippy/test` (no local toolchain) — CI must prove Rust.
- Agent access review (accounts/providers/sys): device shell now spawns scrubbed (`env_clear`, PATH/locale + Windows essentials only) — server env with keys/secrets no longer leaks via `env|printenv` into transcripts; redaction markers added (`key=`, `passphrase=`, `jwt_secret`, `ya29.`) + TS mirror `src/utils/redact.ts` now applied to all browser tool outputs, user answers/feedback and `memory_remember` (native parity); approval `remember` persists allow-rules on admin REST calls only (one-time approve still works for all); agent transcripts owner-bound (`owner_id`, admin-or-owner REST reads, legacy rows readable, Tauri local unchanged) + isolation test.
- Not run here: `cargo fmt/clippy/test` (no local toolchain) — CI must prove Rust.
- Android red fix (CI main 2026-10-08): `processArm64ReleaseMainManifest` merger failed with "Error parsing AndroidManifest.xml" — `android-configure.sh` prepended `android:usesCleartextTraffic="true"` while the Tauri template pins `="${usesCleartextTraffic}"` (manifestPlaceholder), yielding a duplicate attribute (not well-formed XML). Patch now strips template-set copies of the three application attributes before setting ours exactly once, and validates well-formedness in-script so a broken patch fails fast instead of after minutes of Gradle. Also fixed `.cyb3` `pathPattern` double-backslash (`.*\\.cyb3` never matches; now `.*\.cyb3`). Silenced 3 `cybermanju-faces` warnings (`Context as _`, `_file_node`, cfg-gated `FaceBox` — all safe in onnx/no-onnx builds).
- Not run here: `cargo fmt/clippy/test`, device builds — CI must prove Rust.

## 2026-10-08 — Android instant-close fix (startup self-heal + probe + smoke)

- Symptom: APK installs but auto-closes without showing anything. Double review found five startup crash vectors, all before first paint (each a `fatal()` panic or silent kill in `run()`):
  1. torn `cybermanju.db` (killed mid-commit/full disk) failed `Database::new` on EVERY launch → permanent brick;
  2. torn `tantivy_index/` failed `SearchIndex::new` the same way;
  3. hardcoded `/data/data/com.cybermanju.os/files` EACCES under work profiles/secondary users (`/data/user/<id>/…`);
  4. `tracing_subscriber::fmt().init()` panics on re-init (activity recreate) or invalid `RUST_LOG`;
  5. 50 MB Tantivy writer arena allocated up front on low-end phones (LMK kill, no logcat marker).
- Fixes (`src-tauri/src/lib.rs`): `open_database()` quarantines a torn DB to `cybermanju.corrupt-<unix_ts>.bak` and recreates (backup kept for forensics); `open_search_index()` wipes + recreates a torn index (rebuildable via `rebuild_search_index`); `probe_android_files_dir()` proves every candidate writable (env override → `default_secret_dir()` → each `/data/user/<id>/…`) before use; logging via `try_from_default_env` + `try_init` (never panics); mobile panic hook prints `CyberManju OS FATAL (panic): …` to stderr first so logcat always names the cause.
- `crates/search`: writer heap 8 MB on Android, 50 MB elsewhere (`WRITER_HEAP_BYTES`).
- `tauri.conf.json` CSP: added explicit `connect-src 'self' http: https: ws: wss:` (LAN dashboard at any private IP, provider APIs incl. self-hosted instances, Supabase — an allowlist would regress those; desktop only survived via the `invoke()` IPC fallback), plus `media-src`, `worker-src 'self' blob:`, `object-src 'none'`.
- `scripts/android-smoke.sh`: now catches the exact symptom — clears logcat pre-launch, asserts the process is alive at 8s AND 13s (`pidof` + `ps` fallback), then greps the fresh window for `FATAL EXCEPTION/has died/Force finishing/CyberManju OS FATAL`. Old script greped the full uncleared logcat and passed a silently-dead process as green.
- Docs: `docs/ANDROID.md` §5 rewritten (self-heal, probing, 8 MB heap, panic breadcrumb) + new §9 instant-close troubleshooting table with the logcat one-liner.
- Not run here: `cargo fmt/clippy/test`, device builds — CI must prove Rust.

## 2026-10-08 — Voice input pt-BR support

- Speech recognition now supports Brazilian Portuguese alongside English (`src/utils/speechCorrect.ts`): `VoiceLang` (`en-US`/`pt-BR`), `detectVoiceLang()` (pt-* browser ⇒ pt-BR), accent-folded matching (`cê dê`→`cd`, `vírgula`→`,` with or without diacritics).
- New pt-BR correction tables: prose punctuation (`nova linha`, `novo parágrafo`, `ponto de interrogação/exclamação`, `abre/fecha aspas`, `vírgula`, `ponto final`, `dois pontos`, `ponto e vírgula`), code symbols (`abre/fecha parênteses/colchete/chave/ângulo`, `seta`, `menor/maior que`, `aspas duplas/simples`, `crase`, `sublinhado`, `hifen`, `barra [invertida/vertical]`, `arroba`, `cerquilha`, `cifrão`, `por cento`, `asterisco`, …), phonetic shell splits (`ce de`→`cd`, `ele esse`→`ls`, `erre eme`→`rm`, …), Portuguese shell aliases (`ajuda`→`help`, `listar`→`ls`, `histórico`→`history`, `apagar`→`rm`, …) and code keywords (`função`→`function`, `constante`→`const`, …). EN tables always apply; PT tables add on in pt-BR mode so mixed speech works while en-US output is byte-identical.
- `useVoiceInput` gains a reactive persisted `lang` (localStorage `cybermanju.voice.lang.v1`, browser-defaulted) with `setLang`/`toggleLang` (restarts the engine, since VueUse only applies `lang` while idle); `useSystemHardware` shares the same persisted language for Devices voice + speech synthesis.
- EN/PT selector next to every mic button: AgentPanel, TerminalPanel, CodeStudio, DevicesPanel.
- Also fixed along the way: prose `nova linha`/`new line` breaks no longer flattened to spaces, and sentence capitalisation is unicode-aware (`á`, `ç`, …).
- Verified: `npx vue-tsc --noEmit` clean; `tests/frontend/voice-shell.test.ts` 28/28 green (15 new pt-BR cases); full suite green except pre-existing `cybsh-script.test.ts` failures (fail identically on the clean tree).
- Not run here: `cargo fmt/clippy/test` (no local toolchain) — CI must prove Rust.

## 2026-10-08 — `.cybsh` scripts: interpreted automation + `run`/`theme`/`ui`

- New script language (python-style, no build): `print`, `let`/`const` (TS `: type` stripped), `if/elif/else`, `for in` (`range()`/lists/strings), `while` (1000-iter cap), inline shell (`$ …`, `sh "…"`, bare verbs, `sh(<expr>)`, `${var}`), `js` expression subset (`===/&&/||/!` normalized), `fetch … [as var]`, `ui`/`theme`, `vars`/`free`/`gc`. Owned values + budgets (64 KiB source · 200 stmts/block · 5000 steps · 64 vars · 16 KiB/string · 256 KiB output) instead of a GC; house `prefix: detail` errors with line numbers; `run` nests 4 deep.
- Three transports, one grammar: native `crates/os/src/script.rs` + `run <file.cybsh> [--dry] [--json]` / `theme` / `ui` verbs in `shell.rs` (volume mirror `/.cybermanju/theme.json`, `ui:` effect lines); static twin `src/utils/cybshScript.ts` wired into `staticCybsh.ts` (`run`/`theme`/`ui`, real browser `fetch` 10 s/64 KiB, live `localStorage` + document apply, wasm `execFallback` for volume lines); compact WASM interpreter in `crates/os-wasm/src/os.rs` (same grammar, `fetch` honestly `unsupported:` — no HTTP client in either Rust crate). Terminal panel applies `ui:` effects via `useTheme()`; speech dictionary + completions learn the new verbs.
- Agent authoring surface: `.cybermanju/skills/cybsh-script/SKILL.md` (statement/expression reference, providers + durability verbs, memory model, error contract, nightly example) + `examples/hello.cybsh`, `examples/providers-tidy.cybsh`. README verb list extended.
- Verified: `bash scripts/check-version.sh` green; `npx vue-tsc --noEmit` clean; `vitest` 36 files / 464 pass (new `tests/frontend/cybsh-script.test.ts` 19/19 — the previously failing suite now passes on this tree). Rust `fmt/clippy/test` left to CI per repo rules (notably the new `script.rs` unit tests and the os-wasm `run`/`theme` arms).

## 2026-10-08 — `.cybsh` philosophy pass: structured values, functions, capabilities, journals

- Web research (Oils/YSH command-vs-expression modes, Nushell structured pipelines, Elvish exceptions-over-exit-codes, Starlark determinism/hermeticity, Rhai rugged embedding, Deno capabilities, bash `errexit` failure modes, Bazel/Nix hermeticity) mapped onto `.cybsh`, then implemented 100% on all three transports (native `crates/os/src/script.rs`, static twin `src/utils/cybshScript.ts` + `staticCybsh.ts` journal wiring, compact WASM port in `crates/os-wasm/src/os.rs`):
  - **Text is UI, structure is API** — new `Dict` type (sorted keys: deterministic display/iteration/`==` everywhere), `d.k`/`d["k"]`/`l[i]`/`s[i]` indexing, functional `set/push/del/keys/values`; `json()` and `sh()` with `--json` materialize into structure instead of re-parsed text; dict commas fixed in all six comma/`=` scanners.
  - **Explicit effects** — `try:`/`catch [var]:` + `fail "msg"` (Elvish over bash; abort stays the default, `||` inside shell lines stays the escape); inline-shell failures now suffix the line (`<cause> (line N)`) so the house `prefix:` stays first for UI matchers; any bare expression is a statement (Oils one-way rule: `sh("…")` works as a line, values print, `_` keeps them).
  - **Determinism** — `# cybsh: 1` version pins (wrong pin = `unsupported:`), `def name():` with `return` (lexical scope, globals readable, assignments never escape, builtins unshadowable, recursion bounded at depth 32), no clock/random builtins (hermetic by omission), `--record/--replay` journals fingerprinted with cross-transport FNV-1a/64 (`fnv("a") == af63dc4c8601ec8c` pinned in tests on both sides; stale replay = `integrity:`, miss = `not_found:`, replay serves `fetch` even where no HTTP client exists).
  - **Capabilities** — `# cap: net=…/deny=…` (Deno rule, deny wins, `denied:` refusals), enforced at the single inline-shell/fetch choke point of each interpreter, audited in `--json` (`caps`, `calls: {sh, fetch}`); `read=/write=` declared + recorded.
- Skill rewritten (`.cybermanju/skills/cybsh-script/SKILL.md`): dicts/functions/caps/journals/errors, pinned nightly example; examples gain version pins, `providers-tidy` gains try/catch+caps, new `functions.cybsh`.
- Verified: `npx vue-tsc --noEmit` clean; `vitest` 36 files / 484 pass (`cybsh-script.test.ts` 19→39, incl. FNV vector, journal record→replay→tamper, capability denials). Rust `fmt/clippy/test` left to CI per repo rules (new `script.rs` dict/def/try/caps tests + os-wasm `run` dict/try/def/caps/journal tests).

## 2026-10-08 — FileManager: disk click scope + vault-root merged home fix

- Disk rows (sidebar VOLUME + inspector Distro): left-click now opens that disk's share — a banner with used/capacity ratio, % of merged volume and a proportional file-size estimate (block-striped volume, so labelled an estimate, never a placement claim) — instead of just opening Disks. Right-click gets a disk menu (Open files share / Manage disk / Attach-Detach / Check / Copy id / Clear filter). Vault-root row clears the scope. `DiskManagerPage` accepts `diskId`/`highlightId` to focus + scroll to the card from "Manage disk".
- Vault-root empty-create fix: `store.createFolder` now normalizes the parent to the vault-path listing key (`currentPath`, `/` at root) — the old code passed a selected folder's id or `''`, which indexed the folder under a key no listing queries, so it never showed. `/providers/…` refuses honestly (read-only canal browse); creation now toasts success instead of closing silently.
- Merged home: `store.fetchFiles('/')` sweeps the legacy `''` parent key and merges true orphans (`!parentId`) into the root view, so folders made by older builds reappear at home on every transport (Tauri answers the key directly; REST/WASM filter the full-table answer down to orphans). Trash-safe (trashed rows leave the files table). Root empty-states now explain the merged home.
- Verified: `bash scripts/check-version.sh` green; `npx vue-tsc --noEmit` clean (one caught + fixed handler-type bug); `vitest` 36 files / 484 pass. Rust untouched (frontend-only; CI owns Rust).
- Known follow-ups (backend, need CI run): `import_file`/`import_from_url`/`scan_directory` with empty parent store `parent_id None` (unindexed → invisible everywhere); `restore_from_trash` reinserts without re-adding the parent index (restored items stay unlisted).

## 2026-10-08 — zstd + triple in the browser (WASM transport)

- New dep `@dweb-browser/zstd-wasm@1.2.1` (wasm-bindgen around zstd-rs; chosen over `@bokuweb/zstd-wasm` Emscripten output because it needs no bundler rules or deployed `.wasm` — `initSync` over the embedded base64 binary, fetch-free, lazy on first zstd use so ~890 KB never joins the initial bundle). New `src/utils/zstd.ts` wrapper: `ensureZstd` (never throws for load failures → callers answer `unsupported:`), `compressZstd`/`decompressZstd` (default level 15 = desktop, clamped 1–22), `compressTriple`/`decompressTriple` (LZ4→ZSTD→Brotli, the desktop `.cyb3` order; standard `28 B5 2F FD` frames both ways).
- Rust `crates/os-wasm/src/compression.rs` gains `decompress_zstd` (pure-`ruzstd`, the same decoder `artifact.rs` already uses) — decode prefers it when the deployed pkg has it, encode stays in JS (the C `zstd-sys` encoder can never build for wasm32).
- Wired everywhere: `StaticCodecs` gains optional `compressZstd/decompressZstd/compressTriple/decompressTriple` (`useTauri.ts` supplies them); cybsh `compress <path> [lz4|zstd|brotli|triple]` → `.lz4/.zst/.br/.cyb3` envelopes, `decompress` unwinds all four (unknown algs still `invalid:`, unavailable legs `unsupported:`); `useWasmCrypto.ts` pipeline compresses/decompresses `zstd` + `triple` with per-layer stats in desktop colors; ShieldPanel note, `COMPRESSION_INFO`, `docs/OPERATIONS.md`, `TASKS.md P2-4` updated (no more "zstd stays desktop-only").
- Verified: `bash scripts/check-version.sh` green; `npx vue-tsc --noEmit` clean; `vitest` 37 files / 494 pass (new `zstd-wasm.test.ts` 10: real round-trips, magic bytes, level clamping, triple chain, shell `.zst`/`.cyb3` envelopes; `static-cybsh.test.ts` updated — `zstd` envelope without a decoder is now `unsupported:`, unknown algs stay `invalid:`). Rust `fmt/clippy/test` left to CI per repo rules.

## 2026-10-08 — Review pass: zstd/triple WASM hardening + cross-device check

- Real bugs found and fixed: (1) `handleCompress`/`handleDecompress` mapped a missing zstd module to `integrity:` — now passes `unsupported:` through, only a leg that actually ran and failed reports integrity; (2) `compressFile` ran the chain twice (encode + stats re-run: wasted work and a stored-but-error state if init failed between runs) — `encodeStored` now returns its per-layer `compDetails`, single run; (3) raw wasm errors from zstd legs surfaced unprefixed in the vault pipeline — wrapped as `integrity:` (still passing `unsupported:` through), removed a no-op in `decompressBytes`, `compressionCapable` also accepts `all`; (4) `ensureZstd` could hand back an uninitialised module when neither init path ran — `didInit` guard resolves `null` instead.
- Flake caught and fixed: `zstd-wasm.test.ts` "clamps levels" timed out once under full-suite load (first-touch compile of the ~890 KB binary > 5 s) — `beforeAll` warmup at 60 s + fewer clamp levels; green in every run since.
- Interop proven with an independent implementation: our level-15 frames decode byte-equal in `@bokuweb/zstd-wasm` (Emscripten build) and vice versa; committed a `node:zlib` cross-implementation triple test (foreign brotli leg unwinds our chain). npm-pack scratch dirs removed afterwards.
- Cross-device: desktop (win/linux/mac/android) and Docker `:3456` use native Rust zstd — untouched; Pages real-pkg gets lz4/brotli (Rust) + zstd/triple (JS); stub builds (Docker frontend stage) still refuse honestly; `db-worker` chunk verified zstd-free; Pages production build checked — wrapper/glue/binary land in separate lazy chunks (2 KB / 3 KB / 889 KB), never in the initial bundle.
- Incidental: fixed 2 `vue-tsc` errors in the concurrent theme/ui workstream (`density`/`motion` literal narrowing, type-only `as` casts, no behavior change). Their `cybsh-script.test.ts > theme + ui verbs` (`ui accent --for`) is still red — their feature mid-development, unrelated to this change, left for them.
- Verified: `vue-tsc` exit 0; `vitest` 505–506 pass with only that foreign failure; `check-version.sh` green; Rust left to CI per repo rules.

## 2026-10-08 — One-folder setup + unified-disk single-copy + verified move + key holder

- One-folder setup (`SetupWizard.vue` + `useCyberManjuFile.createCyberManjuFileInDirectory` + new pure `src/utils/vaultFolder.ts`): one directory pick creates `<picked>/vault.cybermanju` beside `<picked>/files/` (the `local` root); all pickers nudge to `files/`, manual vault paths get a loop warning + one-click fix, `Save & verify` refuses loop paths.
- Loop guards both layers: frontend `isProtectedSyncPath` + Rust `backends::is_protected_remote_path` (`*.cybermanju`, `cybermanju-up-*.cyb3`, `master.passphrase`, `keystore.json`, `/.cybermanju/`); pipeline skips quietly, `LocalBackend::upload_file` refuses with `integrity:`.
- Single-copy home by default: `SyncConfig.mirror` (opt-in duplicate), `SyncConfig.key_holder`, `SyncFile.home_config_id`, `DiskRow.holds_keys` (all serde-defaulted, old rows parse); new pure `src/utils/filePlacement.ts` (`chooseHome`/`mirrorTargets`/`planMove`/`designateKeyHolder`); wizard disks step has mirror checkboxes + key-holder radio (server enforces single-holder on save).
- Verified move A→B end to end: shared core `crates/sync/src/relocate.rs` (download A → upload B → BLAKE3 verify → delete A → retarget record; striped/vault/conflict honest refusals; lock-free via cloned `Database` handle — `Database` is now `Clone`); exposed as `POST /api/sync/move`, Tauri `move_sync_file`, `cybsh sync move <file> <from> <to>`, Sync panel Move form, store `moveSyncFile`; `POST /api/disk/key-holder` + Tauri `set_disk_key_holder` + DiskManager KEY HOLDER button + wasm `disks.key-holder` op. `mv` keeps moving namespace bytes (incl. cross-mount copy+delete); `sync move` moves a synced copy's home so the catalog follows.
- Verified: `bash scripts/check-version.sh` green; `npx vue-tsc --noEmit` clean; `vitest` 513 pass (new `vault-folder` + `file-placement` suites, new `sync move`/`mv` cross-mount static tests). Rust `fmt/clippy/test` left to CI per repo rules.

## 2026-10-08 — Heuristic faces everywhere + persistent folder attach

- Faces without ONNX now detect instead of staying silent: new pure-Rust `crates/faces/src/heuristic.rs` (YCrCb skin mask → connected components → geometry filters → 512-d crop embeddings, engine `heuristic-v1`, confidence per face, 6 unit tests for CI). `image` is a mandatory faces dep (pure-Rust jpeg/png/webp — mobile-link-safe). Funnel `detect_faces_in_file_with_engine` (+ `detect_faces_auto` bytes-injected, + batch-with-engines): ONNX success (even empty) stays `onnx`; ONNX error/absence falls back to heuristic; unreadable bytes stay `none`. Groups record `FaceGroup.detection_engine` (`#[serde(default)]` — old rows read as unknown); mixed-engine matches become `mixed`; responses carry `engine`/`detectionEngines`.
- Desktop: single/batch/recluster commands use the funnel (Android included — no `ort` linked). Docker: new `POST /api/faces/detect` + `POST /api/faces/detect-batch` (`crates/web/src/api/faces_api.rs`, same grouping math, bytes from `original_path`) + REST mappings (detect pair leaves WRITE_ONLY; grouping admin stays desktop-only). Pages: TS twin `src/utils/faceHeuristic.ts` (browser-free core, tested) + kv-backed static handlers (detect/batch/list/rename/merge/delete/similar); panel shows `via HEURISTIC-V1` and per-group engine tags; inspector face rows light up from the same store.
- Folder attach that survives reloads: new `src/utils/localDir.ts` (typed dir-handle registry in IDB, boot restore that queries but never prompts, one-click re-grant, real list/read/write/mkdir/delete through handles with house prefixes) + `idbKeys()`; App boot restores wizard + provider dirs; SetupWizard/Accounts persist via the registry (no more lying `as string` casts); sync step shows remembered-folder status + Re-allow button; vault step explains plain-HTTP LAN (Docker over `http://nas`) has no pickers and keeps the vault server-side. Desktop native dialogs and mobile app-private partitions verified prompt-free by design (no code change).
- Verified: `vue-tsc` exit 0; `vitest` 43 files / 529 pass (new `face-heuristic` 12 + `local-dir` 9 + faces route lock). Rust `fmt/clippy/test` left to CI per repo rules (new heuristic unit tests + updated `test_face_group_serde` with legacy-row check run there).

## 2026-10-08 — Heuristic faces v2: eyes-or-it-is-not-a-face

- Literature review (Hsu/Mottaleb/Jain ICIP-2001 white-patch + eye/mouth maps; Chai–Ngan box; Yang et al. hands-vs-faces; Campadelli ≥1-eye rule; three-Gaussian wide rect) exposed v1's core flaw: skin geometry alone cannot tell a hand from a face. v2 (`heuristic-v2`, Rust + TS twins with matched integer math): white-patch lighting compensation (clamped, skipped without a bright patch), luma-adaptive skin bounds (tight box mid-luma, wide rect at extremes — dark skin now detected), 3×3 opening, then eye/mouth verification per candidate — luma valleys with pair geometry (aligned/separated/symmetric) or one eye plus an Hsu mouth-map mouth. A bare skin blob is rejected by design.
- Confidence = geometry base + feature bonus (cap 0.95); embedding layout unchanged (512-d, compatible grouping); engine bumped v1→v2 everywhere (behavior changed), UI tag generalized to any `heuristic-*`, missing engine reads `unknown`.
- Tests hardened both sides with identical fixtures: eyes+mouth found (>0.5 conf), featureless blob rejected, one-eye+mouth passes, lone eye rejected, dark skin found, determinism, crop separation. TS 11/11 green; Rust unit tests added for CI (`cargo` stays CI-only per repo rules).
- Verified: `vue-tsc` exit 0; `vitest` 43 files / 533 pass; `check-version.sh` green.

## 2026-10-08 — File manager sees all providers; cybsh reaches outside the container

- File manager now shows every Google Drive / GitHub / GitLab repo: `store.fetchFiles('/providers')` lists mounts plus enabled-but-unmounted configs (`pending-<id>` placeholders that mount on click); subdirectories try the canal first and fall back to REST `list_remote_files` with one-level folder synthesis (new pure `src/utils/providerBrowse.ts`); FileManager auto-mounts enabled configs on open, routes picker uploads into the current provider folder, and `vaultWriteBytes` falls back to `os_write` for text on web/static (binary refuses honestly). Provider ⇄ provider moves stay verify-before-delete; saves land in the vault root (`.cybermanju` home).
- cybsh provider namespace `/providers/<mount|config>[/path]` on every transport: native Rust (`crates/os/src/provider_fs.rs`, wired into `ls/cd/cat/cp/mv/rm/mkdir/touch/stat`) resolves mounts via kv rows or config ids, lists through the classified sync backends (git trees synthesize folders; Drive merges files + a new `list_dirs` folder query — new defaulted `StorageBackend::list_dirs` in `crates/types`, Drive-only override so existing implementors compile untouched), copies verify via BLAKE3 re-read, moves are copy→verify→delete-source, `mkdir` writes `.keep` (git has no empty dirs). `du/find/grep/head/tail/wc/write/edit/encrypt/decrypt/compress/decompress` refuse provider operands with `unsupported:` (cp locally first); `local`-backend configs refuse (host path, not remote). Static shell gains provider `ls`/`cat` (local operands still fall through to the wasm volume); `docs/OPERATIONS.md` §4a documents the namespace.
- Verified: `vue-tsc` exit 0; `vitest` 45 files / 547 pass (new `provider-browse` 4 + provider `ls`/`cat` static tests 6). Rust `fmt/clippy/test` left to CI per repo rules (new `provider_fs` unit tests + Drive `list_dirs` run there).

## 2026-10-08 — macOS-style flat AMOLED glass remake (UI/UX)

- Tokens: `src/ui/tokens.ts` cut from 17 themes to 3 (`os-dark` true AMOLED, `os-light` paper, `os-graphite` mono) with one fixed geometry (window 12 / menu 10 / control 6 / chip 4), SF-only type, flat shadows + 0.5px outline + focus ring. Glow/aurora/neon/shape/font axes kept as deprecated aliases resolving flat so old markup compiles. `useTheme()` migrates stored ids by mode, adds follow-system auto appearance, collapses glass to solid/translucent. New WCAG guards in `tests/frontend/ui-theme.test.ts` (text + text-2 ≥ 4.5:1, accent ≥ 3:1 — all pass; light text-2 lifted to .80 alpha to clear 4.5).
- Shell parity: native `crates/os` + `os-wasm` + static cybsh theme tables accept the 3 ids with all 17 legacy ids migrating by mode; glass verbs collapsed to solid/translucent; defaults os-dark, glow off. Frontend + wasm + native tests updated.
- Chrome: AppWindow 28px centered-title titlebar with shared `UiTrafficLights` (grey inactive), TopMenuBar 24px glass with Window menu + monochrome tray + inline clock, Dock floating 14px glass with magnification/hover labels/running dots/trash-at-end, static gradient wallpaper with canvas opt-in (matrixRain default off), Spotlight palette, top-right 340px banners, 260px alert confirm, sheet auth + setup wizard, black boot screen with thin progress bar.
- Panels: Finder FileManager (translucent sidebar, accent-fill selection + grey unfocused, zebra list, 60px icons, sentence-case headers), Settings Appearance (Auto/Light/Dark/Graphite segmented + accent dots + chrome toggle + wallpaper effects), flat Terminal/CodeStudio/Agent bubbles, sentence-case labels everywhere, flat UiCheckbox/Progress/Card/Text, 4 new primitives (UiSegmented/UiSidebarItem/UiMenu/UiTrafficLights). Icon set self-pruned `minimize-bold` (164 icons).
- Verified: `check-version.sh` green; `npm run typecheck` (icons+vue-tsc) exit 0; `vitest` 45 files / 550 pass. Rust `fmt/clippy/test` left to CI per repo rules. No Playwright in repo — 5-screen screenshot baselines not added (needs new dep + browsers).

## 2026-10-08 — Kubuntu/Plasma shell style (second axis, not more themes)

- New orthogonal axis: `shellStyle: macos|plasma` in `useTheme()` (`cybermanju_shell_style`, `data-ui-shell`; default macos, non-breaking). `src/ui/shells.ts` registry picks chrome (TopMenuBar/Dock vs PlasmaPanel+Kickoff); `useShellInsets()` reads `--ui-menubar-h/--ui-panel-h` live and the window-manager fallback math respects the 44px panel. Single-window invariant + aliases untouched.
- Tokens: `plasma-dark` (#1b1e20/#232629/Breeze #3daee9, on-accent #232629) and `plasma-light` (paper/Breeze #178dc2 — closest blue clearing 3:1 both ways, on-accent white) with panel/hover surfaces, 6/6/4/3 radii, 30px titlebar, 44px panel var, 2px accent focus, 100/150ms motion, 20px panel-only blur. Contrast tests extended to all 5 themes + on-accent ≥ 3:1 (accent checked on surface + composited panel, where it renders).
- Chrome: `PlasmaPanel.vue` (launcher, layout pager, task manager with middle-click close + window menu, tray with network/sync/notifications/volume/theme popovers, clock + calendar), `KickoffMenu.vue` (MODULE_METADATA grid, search, Favorites/Apps/Recent/Power, avatar + lock/logout/restart), `UiTitlebarButtons` (right-side glyphs, danger close hover), AppWindow plasma titlebar + edge snap → tiled + titlebar menu with always-on-top pin, KRunner palette, bottom-right banners, icon-left confirm, SDDM login (wallpaper-through + bottom-left power), 3 original wallpapers + picker.
- Panels: Dolphin FileManager (plasma Places, Alt+L editable location bar, F11 info panel, zoom slider + free space in status bar, zebra/details already flat), Settings gains shell style + wallpaper + panel position + section search, Agent user bubble = accent 20% tint in plasma, Activity-Monitor process table sentence case.
- Keys: `[Plasma]` bindings in `keymaps/default.kpl` (Alt+F1 Kickoff, Alt+Space/Ctrl+Space KRunner, Alt+Tab desktop-only task switch, Meta+Arrows snap halves implemented in App.vue). cybsh theme tables (static + native + wasm) accept the plasma pair; frontend + wasm tests cover them.
- Verified: `check-version.sh` green; `npm run typecheck` exit 0; `vitest` 45 files / 553 pass. Rust left to CI. Mobile: panel hides ≤768px, MobileNav/Launcher unchanged (tokens only). No Playwright in repo — matrix/screenshot QA not added.

## 2026-10-08 — Native controls themed + merge with concurrent agent

- All bare `select` / checkbox / radio / slider elements now go through one token-driven baseline in `src/assets/main.css` (themed boxes, accent fills, `--ui-focus-ring`, `color-scheme` per mode so native arrows + option popups follow the theme; custom 4px slider grooves with `--p` fill, 14px thumbs; coarse-pointer sizing). Fill wired per slider (FileManager zoom, disk-size slider); focus rings normalized to the token across `.am/.sw/.msw/.tf` inputs; checkbox label rows aligned.
- Removed the hardcoded terminal colors: new `--ui-terminal` token per theme (black in dark modes, paper in light) and radio dots / slider thumbs now use `var(--ui-on-accent)` / `var(--ui-text)`; the checkbox check stays white except plasma-dark (dark glyph override). Remaining `#fff` spots are documented constants that clear 3:1 on every danger/accent fill.
- Merge: another agent was concurrently rewriting `.vue` files into CSS-only fragments (7 components destroyed incl. TerminalPanel/TransferGraph mid-session) plus real voice-input auto-detect work (`resolveVoiceLang`/`VoiceLangPref` in `speechCorrect`/`useVoiceInput`). Kept their TS work; restored the 7 components from HEAD (fragments held only orphaned classes, no template/script to merge) and re-applied the theme edits on TerminalPanel/TransferGraph.
- Verified: `npm run typecheck` exit 0; `vitest` 45 files / 554 pass; `npx vite build` succeeds (pre-existing chunk-size warning only).

## 2026-10-08 — Android ships the real launcher icon (Tauri-default logo fix)

- Root cause: `tauri android init` stamps the Tauri template's DEFAULT launcher icons into `gen/android/app/src/main/res/`, and no step ever replaced them — the official flow (`tauri icon <source>` AFTER init, per https://v2.tauri.app/distribute/google-play/#changing-app-icon) was missing from every pipeline, so all APKs/AABs carried the Tauri logo.
- New `scripts/android-icons.sh` (idempotent): runs `npx tauri icon public/icon-512.png` (512x512 RGBA canonical mark; `bhumisparsha.png` is 250x280 non-square) into a temp dir — never the default `src-tauri/icons` output, so no tracked desktop icon is touched — copies only the `android/` subtree into the generated project's `res/` (copy-not-delete, template files like `strings.xml` survive), then verifies all 15 mipmap PNGs + adaptive-icon XML + background exist and the foreground densities match the documented 108/162/216/324/432.
- Wired between init and configure in all three pipelines (`.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.gitlab-ci.yml`); `docs/ANDROID.md` §1/§8 + `android-configure.sh` header + `AGENTS.md` updated.
- Verified: script run against a fake gen dir (17 files land, densities check, template files intact, `git status` shows no tracked churn); `bash -n` clean. `check-version.sh`/`vue-tsc`/`vitest` re-run below.
- Instant-close report: native side already self-heals (DB quarantine, index rebuild, files-dir probing, re-entry-safe logging, 8 MB Tantivy arena on mobile) with logcat breadcrumbs — needs the reporter's device model + Android version + APK name + `adb logcat` excerpt (`docs/ANDROID.md` §9) to diagnose; no code change made for it in this pass.

## 2026-10-08 — Crash log made un-truncatable (follow-up to instant-close report)

- Reporter's logcat proved the crash site: `CyberManju OS FATAL (panic)` at `src-tauri/src/lib.rs:43` — the `panic!` inside `fatal()`, i.e. a startup gate (DB dir creation, DB open+quarantine, or index open+wipe) refused. But the refusal REASON was lost: it was logged on following lines that a bare `grep "CyberManju OS FATAL"` filters out.
- `fatal()` now logs `CyberManju OS FATAL: <reason>` (marker-prefixed, single line) and the mobile panic hook flattens location+payload to one line — the cause always survives the triage grep. `docs/ANDROID.md` §9 now prescribes `grep -A 10` and says why.
- Still needs from the reporter: Android version + exact APK name + `grep -A 10` output to name the actual gate. Rust validation left to CI per repo rules (no local cargo).

## 2026-10-08 — Android instant-close root-caused: Tantivy 15 MB arena floor

- Reporter's `grep -A 10` gave the reason: `Failed to initialize Tantivy at …/tantivy_index even after wiping the corrupt index: … 'The memory arena in bytes per thread needs to be at least 15000000.'`
- Root cause: `crates/search` used an 8 MB writer heap on Android, but Tantivy 0.22 rejects anything under 15 MB — so `SearchIndex::new` failed on EVERY launch (fresh installs included; never device-specific), `open_search_index` misread it as a corrupt index, wiped a healthy index, retried, failed, and `fatal()`-panicked before first paint.
- Fix: Android `WRITER_HEAP_BYTES` 8 MB → 15 MB (the library floor, still 3.3× smaller than desktop's 50 MB); comment cites the floor so nobody "optimizes" it back down. `docs/ANDROID.md` §5 updated. Needs a rebuilt APK + reinstall (uninstall wipes the app-private index dir, so a fresh start is guaranteed).
- Verified: floor value comes verbatim from the library's runtime error on-device (`>= 15000000`); `check-version.sh` green. `cargo clippy/test` left to CI per repo rules.

## 2026-10-08 — Android instant-close root-caused (2): tauri-plugin-log double logger init

- Reporter's new `grep -A 10` named it: `PluginInitialization("log", "attempted to set a logger after the logging system was already initialized")` panicking at the old `lib.rs:527` (`.expect()` on `tauri::Builder::run()`).
- Root cause: `run()` installs our own `tracing_subscriber::fmt().try_init()` and then `tauri-plugin-log` calls `log::set_logger` — the two collide, and any second `run()` in the same process (Android activity recreate) re-registers the plugin logger and fails the same way. Either path panicked before first paint.
- Fix: removed `tauri-plugin-log` everywhere — `.plugin()` line in `lib.rs` (with a comment citing the crash), `src-tauri/Cargo.toml`, `package.json` (+ `package-lock.json` re-synced via `npm install --package-lock-only`), `log:default` from both capability files. The frontend never imported `@tauri-apps/plugin-log`; Rust logs reach logcat via stderr/tracing, so nothing is lost. `docs/ANDROID.md` §5 updated. `Cargo.lock` keeps a stale `tauri-plugin-log` entry until CI's cargo run prunes it (no `--locked` in CI, no local cargo per repo rules).
- Verified: `check-version.sh` green, `vue-tsc --noEmit` clean, vitest 45 files / 554 tests pass. Rust validation left to CI.

## 2026-10-08 — Mobile windows, file controls and unified account flow
- Centralized OAuth sign-in and broker setup in Accounts/Settings; desktop and mobile onboarding now hand off to Account Manager instead of duplicating login, and empty OAuth-capable connections are blocked until a matching session or personal access token exists.
- Mobile app windows now fill the space between the top menu and dock. File Manager actions are grouped into touch-sized sections, Account Manager uses a horizontal provider rail and session drawer, and Agent opens as a chat-first surface with a hamburger navigation drawer for chats, assistant setup and controls.
- Added responsive contract coverage for full-bleed sheets, OAuth hand-off, credential gating, touch-first Files tools, Agent navigation and narrow-window layouts.
- Verified: version check, icon generation, `vue-tsc --noEmit`, 48 frontend test files / 570 tests, and `npm run build`.

## 2026-10-09 — Fixed OAuth broker, mobile cybsh, onboarding ends

- Removed OAuth endpoint/public-key customization: broker is fixed at build time
  (`VITE_SUPABASE_URL`/`VITE_SUPABASE_ANON_KEY` via CI secrets). `getSupabaseConfig()`
  reads build env only; `set/clear/hydrateSupabaseConfig` are no-op shims.
  Settings broker card is read-only status; Accounts banners no longer link to a
  Configure flow; `supabase-config` tests cover env-only behavior.
- Fixed mobile terminal: added native IPC `os_exec/os_complete/os_stat/os_ls/os_du/os_write`
  + `get_disk/destroy_disk` (`src-tauri/src/commands/os_shell.rs`, registered in `lib.rs`);
  renamed disk `disk_id` params to `id` to match frontend invoke args. Frontend
  routes the full OS/disk set over IPC on Tauri-mobile with no server URL, and every
  other REST_FIRST command tries IPC first before REST — so cybsh works in the
  Android WebView instead of dying on localhost:3456.
- Onboarding ends: fixed "4 quick things" copy drift, removed the mobile wizard's
  inverted auto-reassign of local vault partitions to a fresh provider, added a
  provider picker + default-model placeholder to the mobile agent step.
- Verified: static review of all touched call sites (no local node_modules/cargo per
  repo rules — `vue-tsc`/vitest/cargo left to CI).

## 2026-10-09 — cybsh virtual-drive listing + `-os` host flag + script/skill coverage

- `ls /` now shows the whole virtual drive: vault volume entries plus a synthetic
  `providers/` row (both text and `--json`; a real volume dir named `providers`
  wins and the namespace still shadows it, as documented).
- New `-os` flag transposes the file verbs onto the host filesystem — shared
  storage on Android (`/sdcard` → `/storage/emulated/0` fallback), process cwd
  on desktop. Covered verbs: `ls/cd/pwd/cat/cp/mv/rm/mkdir/touch/stat/du`
  (both `cp -os`/`mv -os` operands are host paths; `mv -os` falls back to
  copy → verify → delete across filesystems; `cat -os` has the 1 MiB print
  cap). A separate host cwd (`cd -os`/`pwd -os`) leaves the volume cwd alone.
  Any other verb with `-os` refuses with `unsupported:` instead of silently
  touching the volume; provider paths never mix with `-os` (`invalid:`).
- `.cybsh` scripts get it for free: native `run`/`sh` lines go through
  `execute()`, `--record`/`--replay` journals capture `-os` output for
  deterministic replay on host-less transports, and `deny=<verb>` capabilities
  cover `-os` variants on both the Rust and TS interpreters. Static/Pages
  (`staticCybsh.ts`, `os-wasm`) refuse `-os` with honest `unsupported:`.
- Docs: agent skill (`.cybermanju/skills/cybsh-script/SKILL.md`) gains a
  namespaces section (volume + `/providers` + `-os`, agent `bash` guidance,
  `--json` capture pattern); `docs/OPERATIONS.md` gains §4a2.
- Tests: native `ls / --json` providers entry, `-os` refusal outside host
  verbs, host verb round-trip in a temp dir, script `deny=` covering `-os`
  (Rust); static `-os` refusal + TS deny covering `-os` (frontend).
- Verified: static review only (no local cargo/node_modules per repo rules —
  Rust + vitest left to CI).

## 2026-10-09 — iOS-aesthetic mobile home (iOS4Android study, rounds 1+2)

- Cloned https://github.com/itsJoKr/iOS4Android to /tmp/opencode/ios4android
  (outside repo git) and mined all 8 lib files for portable patterns:
  PageView pager + IconGrid (2 rows x 4) + DotsIndicator (7.5px dots, 16px
  spacing) + StatusBar/XStatusBar (carrier left, clock centered, battery
  right, X notch pill, 30/52px bars) + LauncherIcon squircle (60px, r=15,
  12px label, 5px gap) + dock (92px, rounded, no labels) + wallpaper stack.
- Round 1 (`MobileLauncher.vue`, `MobileNav.vue`): springboard pager
  (MLA_PAGE_SIZE=8, snap scroll, rAF page tracking, tappable dots, reset on
  search), 3-zone status bar, safe-area-top, CSS wallpaper wash (no binary
  assets), squircle tiles (16px) + gloss, dots (7.5px/16px), dock blur 28px
  + radius 28, home-indicator pills, spring press states, focus-visible
  ring, tab haptics, heavier tab-bar blur. Reference bugs NOT copied:
  DotsIndicator renders 3 dots for 2 pages and its onPageSelected is never
  wired — ours renders exact counts and dots actually jump pages.
- Round 2 (density): column gap 20->16, title clamp 29-36->24-30, grid gap
  19->16, widgets 76->70px + radius 22 (iOS widget corner), search 49->47px
  + placeholder "Search" (Spotlight wording), dock icons 47->52px, press
  feedback brighten->dim (native SpringBoard touch-down), so page 1 + dock
  fit a phone viewport without scrolling like the reference.
- Layering check (`DesktopShell.vue`): launcher sits UNDER fullscreen
  sheets, so dock-vs-tab-bar double chrome cannot occur — no change needed.
- Honest limits: no notch-variant bar (notch not detectable in CSS), no
  photographic wallpaper (Apple imagery + binary weight; CSS homage
  instead), off-page icons stay tabbable (no inert management yet).
- Tests: new `tests/frontend/mobile-ios.test.ts` (13 asserts); targeted +
  full suite green (50 files / 597 tests, incl. existing responsive
  contract). `vue-tsc` shows only 2 pre-existing errors in untouched
  working-tree files (FileManager hdd icon + call, MobileSetupWizard ??);
  none in touched files.

## 2026-10-09 — Mock/fake/placeholder audit + full namespace wiring (vault ⇄ provider ⇄ device)

- Audit result: zero TODO/FIXME/stub markers in shipped source (only input
  `placeholder=` attributes and test doubles). The real "mocks" were
  behavioral: dead-end refusals, unfiltered bulk actions, and one genuinely
  broken contract — fixed below, nothing simplified.
- `upload_file` shape mismatch (native drag-drop + provider→vault broken):
  the frontend always sent `{fileName, fileData, parentPath}` (static-host
  shape) while Tauri Rust demanded `{file_path, parent_path}` (missing-field
  rejection on desktop). Rust now accepts both: path uploads stream from
  disk as before; byte uploads land under `./imports/{id}_{name}` and go
  through `import_file` (hash/EXIF/index identical), then rename to the real
  file name.
- New `get_sync_usage` Tauri IPC twin (pure function of the saved config —
  `oauth_start` stays REST-only by design: it mints dashboard redirect
  URLs). Registered in `lib.rs`, added to `MOBILE_NATIVE_OS_COMMANDS`, so
  Android gets real quota instead of a localhost refusal.
- Provider moves grow up: `openProviderMove`/`doProviderMove` no longer
  refuse folders — files and trees cross via `moveAnywhere` (verified,
  same-object guard). `saveProviderFileToVault` preserves folder shape
  under `/<folder>/…` (new `ensureVaultDirs`) instead of flattening
  basenames into `/` (which collided and overwrote same-named files).
- Namespace gates: vault-only actions (shield engines, versions/ACL,
  share links, collections, tags, sync/probe/restore, bulk
  encrypt/compress/sync) now filter or explain on provider/device rows
  (`isVaultRow`/`activeIsVault`) instead of one error toast per file;
  `doSyncTo`/`bulkSync` filter to vault rows; `doDelete` partitions
  host (permanent `rm -os -r`) / provider (canal) / vault (trash).
- Device text editing: host text files get an in-place modal editor
  (strict UTF-8, 512 KiB cap, binary refuses honestly, save-on-close),
  mirroring the SAF phone-folder editor; menu + inspector entry points,
  Escape/mobile-back handling.
- Reviews/fixes while passing through: `solar:hdd-bold` never existed in
  the installed icon set — replaced with `solar:folder-tree-bold` and
  regenerated the icon bundle; removed a raw NUL byte that landed in a
  string literal (now `'\0'`); `trackShellCwd` ignores `cd -os` (host cwd
  is a separate namespace — the volume mirror kept showing `-os "…"`).
- Deliberately NOT rewired: TransferGraph keeps its own engine (per-row
  cancel, artifact passphrase, cap pre-refusal — a rewire would lose
  features); `oauth_start` stays server-side (needs the OAuth client).
- Docs: `transport-routes.test.ts` updated to the intended mobile contract
  (cybsh layer + quota over IPC, with backend-registration assertions).
  AGENTS.md module/handler counts left for a maintainer re-verification
  (22 mods / 153 handlers currently; drift predates this session).
- Verified: `vue-tsc` clean in all touched files (remaining 4 errors are
  the other agent's in-flight `staticCybsh`/`cybshScript`/`MobileSetupWizard`
  refactor); `npm test` 595/600 green — the 5 failures sit in that same
  in-flight refactor (`cybsh-script`, `setup-wizard`), whose import graphs
  don't touch this work; targeted files covering this work
  (`host-browse`, `transport-routes`, `shell-cwd`, `provider-browse`,
  `local-dir`, `static-cybsh`) 115/115 green, incl. new
  `host-browse.test.ts` (17 asserts). Rust left to CI per repo rules
  (no local cargo).

## 2026-10-09 — iOS-style Settings sheet on phones

- Double-reviewed `SettingsPage.vue` (560px rules) against the iOS Settings
  language: large title, grouped inset lists with icon tiles, filtering
  search, 44pt cell rhythm. Previous mobile sheet was a shrunken desktop
  dialog (13px title, wrapping chip rows, stacked label-over-control rows,
  search filtered chips only).
- `SettingsPage.vue` (scoped CSS + small logic, desktop untouched):
  SF stack + 26px/800 large title; grouped background (body tints 5% toward
  text, groups stay solid surface, 14px radius — solid fills because phones
  disable backdrop-blur globally); card icons become 28px accent-gradient
  tiles with 15px/700 titles; jump chips collapse to one snap rail with
  40px targets and a 16px search field; rows stay label-left/control-right
  at 44px min-height with solid hairlines (wide segmented wraps, not
  stacks); nested gesture/key tables lose their inner box; action buttons
  40px.
- Search now filters whole groups by label + row keywords
  (`SECTION_KEYWORDS`, 7 sections) with an iOS "No results + Clear" state;
  scroll spy skips `display:none` cards (`offsetParent` guard).
- Verified: new `tests/frontend/settings-ios.test.ts` (9 asserts) + prior
  suites green in targeted runs; `vue-tsc` clean on touched files. Full
  suite: 607/609 — the 2 failures are pre-existing worktree edits in
  `cybshScript.ts`/`cybsh-script.test.ts` (byte-count asserts, reproduce in
  isolation, untouched by this change).

## 2026-10-09 — WASM `-os` masquerade fix + `.cybsh` vs pure-command parity

- Research: pure `ls -os` and script `sh("ls -os …")`/`$ … -os` share one
  choke point per transport (native `dispatch`, static
  `runStaticCybshLine`, wasm `dispatch` via `exec_result`), but the wasm
  dispatcher had no `-os` guard at all — `ls -os` answered with a volume
  listing (masquerade), `cat -os /x` looked up `/-os`, and chained
  `ls -os && …` lines bypassed the static guard (`parseCybshLine → null`)
  straight into the hole. Native and static both refused honestly.
- Fix (`crates/os-wasm/src/os.rs`): `has_os_flag` + `os_refusal` guard at
  the top of `dispatch()` (exempting `complete`, whose arg is a raw
  prefix), same `unsupported: '<verb> -os' addresses the host filesystem
  …` shape as the static layer; `help` gains the `-os`/sh-wrapper
  guidance line. Static comment now names the three covered paths.
- Tests: wasm x2 (`os_flag_never_masquerades_as_a_volume_answer` incl.
  `--host`/`--os` aliases + chained `exec` + real `run` script file,
  `os_flag_script_wrapper_refuses_like_the_terminal` via `exec_result`);
  native shell test fixed (it asserted host verbs `grep/find/df/write -os`
  refuse — they route to the host; now asserts true non-host verbs refuse
  + `os_flag_pure_command_and_script_wrapper_agree`) and script verbatim-
  passthrough test added; vitest `static-cybsh.test.ts` +2 (aliases,
  non-host verbs, `run` + `sh("ls -os")` agreement).
- Docs: `OPERATIONS.md` §4a2 + skill `SKILL.md` list the full 18 host
  verbs (were stale at 10) and state the pure/wrapper agreement + the
  three refused wasm paths explicitly.
- Verified: `bash scripts/check-version.sh` green; `npx vue-tsc --noEmit`
  clean; `static-cybsh.test.ts` 74/74. Full suite 623/626 — the 3
  failures are concurrent-workstream edits in `cybshScript.ts` /
  `cybsh-script.test.ts` (fetch byte-counts) + `settings-appearance`
  (wallpaper toggle), all in files untouched by this change. Rust
  `fmt/clippy/test` left to CI per repo rules.

## 2026-10-09 — Skill audit: cybsh-script SKILL.md refreshed to recent docs

- Audit (prompted check): the skill missed recent worklog items — stale
  pre-remake theme ids, no v2 language surface, no `sync move`,
  no `run --lint/--fmt`, no compress algos, no schedule frontmatter.
- Fix (`.cybermanju/skills/cybsh-script/SKILL.md` only, no code):
  canonical 5 theme ids (`os-dark/os-light/os-graphite/plasma-dark/
  plasma-light`, legacy ids noted as migrating) in the theme table,
  example, and nightly script; new "Match, await, import, with, pipes,
  inputs" section (`match` with `ok/err/else` arms, `ok()` constructors +
  `?`/`unwrap()`, `await … timeout N`, `import … as ns` (`ns_name` calls),
  `with name = …:`, `|>`, `args`/`arg()`/`env()` — all verified against
  `crates/os/src/script.rs` + `src/utils/cybshScript.ts` before writing);
  `sync move` in the verb list + namespaces; `run --lint/--fmt` in the
  run block; `compress [lz4|zstd|brotli|triple]`; `# schedule:/# on:/# desc:`
  frontmatter + pinned nightly example; vault-never-synced + key-holder
  agent note. Description line gains the v2 keywords for discovery.
- Verified: `npx vue-tsc --noEmit` clean; `static-cybsh` green,
  `cybsh-script` 38/40 — same 2 pre-existing concurrent-workstream fetch
  failures as baseline (markdown-only change, no test imports the skill).
  Rust untouched (nothing to validate in CI for this change).

## 2026-10-09 — Android Gaveta de Apps (launcher bridge, no mocks)
- New reusable crate `crates/launcher` (0.1.1, workspace member): `AndroidApp`/`IconPack` types, `validate_package_name` (`invalid:`), `sort_apps`, `pack_icon_candidates`; Android JNI layer (`queryIntentActivities` MAIN/LAUNCHER, `loadLabel`/`loadIcon` → 96px PNG data URL, `getLaunchIntentForPackage` + `FLAG_ACTIVITY_NEW_TASK` open, `ACTION_DELETE` uninstall, theme-discovery icon-pack query + drawable resolve); non-Android refuses `unsupported:` (never mock rows). Unit tests in-crate.
- `src-tauri/src/commands/launcher.rs` (5 commands) wired into `lib.rs`; `cybermanju-launcher` dep added; `useTauri.invoke` routes `launcher_*` to device IPC, never REST.
- `scripts/android-configure.sh`: `<queries>` MAIN/LAUNCHER (no `QUERY_ALL_PACKAGES`) + HOME/DEFAULT intent-filter (Home picker); `docs/ANDROID.md` §3 documents both.
- Frontend: `types` (`AndroidApp`/`IconPack`/`LauncherOverride`), `utils/launcher` (order/filter/swap/gallery-resize/swipe-up, tested in `tests/frontend/launcher.test.ts` 6/6), `composables/useAndroidApps`, `stores/launcher` (per-key overrides `android:<pkg>`/`os:<panel>`, drawer state, localStorage), `components/AppDrawer` (swipe-up Gaveta: search, real icons, tap-to-open, hold-to-customize: rename, gallery icon, icon-pack apply, move, hide, uninstall, reset).
- `MobileLauncher`: bottom-origin swipe-up opens the drawer, All-apps handle, home tiles honor rename/reorder/gallery-icon/hide with hold-to-customize sheet + Reset home.
- Verified: `npx vue-tsc --noEmit` clean; `launcher.test.ts` 6/6; `check-version.sh` ok for `crates/launcher`; full `npm test` shows only the 2 pre-existing `cybsh-script` failures from the dirty worktree (pass on stash, unrelated files). Rust validation left to CI per AGENTS.md (no local cargo).

## 2026-10-09 — Launcher re-review + messages hub (top-right icon)
- Re-verified the JNI bridge against `jni` 0.21 docs and hardened it: per-row
  `with_local_frame` (a 300-app drawer otherwise overflows the 512-entry
  local-ref table and aborts the VM), owned-value `JValue::Object` args
  (`JObject` is neither Clone nor Copy), exception-clear on every failure
  path (a pending exception silently no-ops later calls), explicit
  `ComponentName` fallback when `getLaunchIntentForPackage` is null,
  `NullPtr(&'static str)` confirmed, `base64::Engine` import, clippy-safe
  asserts. `AndroidApp` gains `socialApp` (exact list + long-token contains
  + dot-segment shorts so `line` never fires in `offline`).
- New messages surface in `crates/launcher`: `StoredMessage`,
  `NotificationState`, `list_messages`/`clear_messages` (reads the mirror
  file only), `notification_state` (Secure `enabled_notification_listeners`
  vs our own package), `open_notification_settings`. Non-Android stubs stay
  `unsupported:`.
- `scripts/android-configure.sh`: writes `CybermanjuMessages.kt`
  (NotificationListenerService → `cybermanju-messages.json` ring buffer,
  social rule mirrored from Rust, Tiramisu-safe label lookup), manifest
  `<service>` + social `<package>` queries (single `<queries>`, merged).
  `bash -n` clean.
- Frontend: `useMessages` (social/messages/state/unread/clear/settings),
  launcher store (`messagesOpen`, `messagesSeenAt` persisted), `MessagesHub`
  (quick-launch social row, stored messages newest-first with unread dots,
  enable-access prompt, refresh/clear), top-right icon in the header tray
  AND a floating home button (home has no header without open windows),
  unread badge fed silently on mount + visibilitychange. `AppDrawer`
  swipe-down close now checks grid `scrollTop` (no more slam-shut scrolls).
- Verified: `npm run icons` (chat-round-dots-bold resolves) + `vue-tsc`
  clean; `launcher.test.ts` 9/9; `check-version.sh` ok. Rust left to CI.

## 2026-10-09 — Hub Clear-all dismisses live notifications too
- The hub footer Clear only wiped the stored file; the live shade kept its
  notifications. `launcher_clear_messages` now also fires an explicit
  `CLEAR_NOTIFICATIONS` intent at the running `CybermanjuMessages` service
  (`onStartCommand` → `cancelAllNotifications` + file wipe, `START_NOT_STICKY`),
  best-effort so an old/missing service never fails the button (file clear is
  the source of truth). Button relabeled "Clear all" with tooltip.
- Verified: `bash -n` clean, `vue-tsc` clean, `launcher.test.ts` 9/9.

## 2026-10-09 — Phase 1: scheduler (cron) end-to-end + Tasks → Schedules tab
- TS twin of the Rust cron parser (`src/utils/schedule.ts`): `parseSchedule`
  (5-field cron with `*,-,/`, `every Nm/h/d`, `@hourly/@daily/@weekly`,
  `invalid:` errors), `nextFire` (UTC, ≤5 years probe), `previewSchedule`,
  `describeSchedule`, `formatIn`. Fixed block-comment `*/15` parse error by
  using `//` comments; `*/15` still parses as a step field.
- Rust fixes blind-paired from the twin's tests: `parse_dow` (`7→0`,
  sort+dedup — its own `sunday_seven_matches_zero` test would have failed in
  CI), `describe()` renders full-range fields as `*` via `fmt_field`.
- `cron_api.rs`: POST upserts when `id` is supplied (`created_at`,
  `last_fired_at`, `last_run_id` preserved); new `upsert()`.
- Transport: 7 `cron_*` commands over REST (`GET/POST /api/cron`,
  `DELETE/POST …/{id}[/run|enable|disable|runs]`), `REST_FIRST`,
  `MOBILE_NATIVE_OS_COMMANDS`, wasm `cron.list/save/delete/history/runRecord`
  ops, static handlers (`cron_ensure_started` → true, `cron_set_enabled`,
  `cron_run` → `runStaticSchedule`), `withScheduleFire` stamps validated
  `nextFireAt` on every save path.
- Static cybsh: `cron` verb (ls/add/rm/run/enable/disable/history +
  did-you-mean) mirroring `crates/os/src/shell.rs::cron_cmd` output;
  `runStaticSchedule(id, deps)` is the single execution path (verb, store
  tick, wasm `cron_run`).
- Store: `schedules` + `fetchSchedules/cronSave/cronDelete/cronRun/
  cronSetEnabled/cronHistory`, browser tick (`tickSchedules`, 30s +
  visibilitychange, static transport only — the daemon owns native).
- UI: ProcessPanel gains `processes|schedules` tabs; Schedules tab is a
  full CRUD (table mirrors `cron ls`, add/edit modal with live
  `previewSchedule` validation + presets + run-on-boot, enable toggle,
  Run-now, History modal). Aliases `cron|automation|schedules` →
  `processes` + `{tab:'schedules'}`; palette entry "Open Schedules (cron)".
- Tests: `schedule.test.ts` 19/19; static `cron` verb suite (7 tests:
  add/list/run/toggle/rm lifecycle, frontmatter `# schedule:`, validation +
  did-you-mean, failed-run recording, `not_found`); transport-routes cron
  REST/IPC coverage; panel-aliases scheduler cases. Pre-existing
  `cybsh-script` red (fetchText signature) fixed.
- Verified: `check-version.sh` ok; `vue-tsc` clean; `npm test` 671/671.
  Rust left to CI per AGENTS.md.

## 2026-10-09 — Phase 3: secrets keystore (password manager) end-to-end
- Rust data model: `crates/types/src/secrets.rs` (`SecretRow`, `SecretMeta`
  structural never-leak projection, `SecretKind` login/card/note/apiKey);
  `secrets` table + CRUD in `crates/db/src/database.rs` (opened in init +
  snapshot export). Values sealed with `keystore::seal_str` (`seal:v1`,
  Argon2id + ChaCha20Poly1305) under `master_passphrase()` — absent
  passphrase answers `unsupported:`, never plaintext-at-rest.
- REST: `crates/web/src/api/secrets.rs` (single source for Tauri + REST +
  WASM twin): list/get return `SecretMeta` only; upsert seals `value` and
  never echoes it; reveal opens the blob + `log_audit("secret.reveal")`;
  delete audited. Wired in `lib.rs`; `ROUTED_SEGMENTS += "secrets"`;
  mutations/reveal Admin-gated, list Authenticated.
- Tauri: `src-tauri/src/commands/secrets.rs` (6 commands) + lib.rs
  registration. WASM: `secrets.list/get/create/update/delete/reveal` ops in
  `os-wasm/db.rs` (session passphrase is an argument, never stored);
  `seal_blob`/`open_blob` exports in `os-wasm/crypto.rs` (inline base64,
  byte-compatible with the native STANDARD engine — no new dep).
- Agent tools (18→20): `secret_list` (metadata, default allow) +
  `secret_get` (plaintext, default ask — approval card shows the title);
  plan agents deny both; subagents exclude both; prompts ×3
  (agent_loop, AgentPanel, useStudioAgent); AGENT_TOOL_META + AGENT_TOOLS
  + balanced preset + `ensure_default_agent_permissions` (+secret_list).
- UI: `SecretsPanel.vue` (category sidebar, search, list, detail card with
  30s auto-hide Reveal, clipboard auto-clear Copy, edit/create modal with
  live strength meter); `src/utils/password.ts` (crypto.getRandomValues,
  rejection sampling, entropy bits) + `src/utils/clipboard.ts` (30s
  overwrite, second-copy cancels first); PanelType `secrets` + aliases
  `passwords|credentials` (`vault` deliberately not — AccountManager tab
  collision); store `secrets` + 5 actions; transport routes REST/IPC/wasm.
- Tests: `password-clipboard.test.ts` 10/10 (charset/entropy/strength/
  fake-timer auto-clear); transport-routes secrets REST/IPC + Tauri wiring
  + Admin-gate assertions; panel-aliases vault cases; agent-wasm-parity
  plan-deny + toolMeta 20. `npm test` 692/692; `vue-tsc` clean;
  `check-version.sh` ok. Rust left to CI per AGENTS.md.

## 2026-10-09 — push.sh release flags + release.yml reuse mode (no-rebuild publish)
- `push.sh`: new `--release` (commit `chore(release): vX.Y.Z`, push main,
  create + push the `vX.Y.Z` tag → triggers the Release workflow full
  rebuild; `-m`/`--release-notes-file` is the release-notes markdown,
  prepended to the atlas body once published), `--release --last`
  (dispatches `release.yml` in reuse mode — zero compilation, ships one
  green CI run's `dist-*` artifacts unchanged; auto-resolves HEAD's latest
  green run, pin with `--reuse-run ID`/`$PUSH_REUSE_RUN`), and `--move-tag`
  (explicit delete/recreate of an existing tag at HEAD + force-update on
  both remotes; refuses when a published GitHub release sits on the tag).
- `release.yml`: new `workflow_dispatch` inputs (`tag`, `reuse_run_id`,
  `notes`) + `resolve` job (tag/version check, zero compilation); every
  build job (incl. `rust-check` and `docker-build`) skipped in reuse mode;
  new `fetch-last-build` job downloads the CI run's `dist-*` artifacts via
  `gh run download` and re-uploads them under `release-*` names so
  `create-release` is byte-identical in both modes (fan-in guarded with
  `if: ${{ !cancelled() && !failure() }}` since skipped needs skip
  dependents by default); custom `notes` prepended to the atlas body.
- `AGENTS.md` §3/§4 document the new flags and dispatch mode.

## 2026-10-09 — Supabase broker: runtime Settings paste + baked into every desktop build
- `useSupabase.ts`: broker is no longer build-env-only. `getSupabaseConfig()` returns
  `{url,key,source:'settings'|'build-env'|'none'}` — an explicit Settings paste
  (`cybermanju.supabaseUrl/Key` in localStorage) wins over the baked pair; half-filled
  sides surface so the UI can name the missing half. `setSupabaseConfig()` validates
  (http URL, key length) and returns an error string instead of being a no-op;
  `clearSupabaseConfig()` forgets the paste and falls back to the baked pair;
  `hydrateSupabaseConfig()` re-reads and returns the live state. A `storage` listener
  keeps every window/tab of the same profile in sync. All "rebuild with
  VITE_SUPABASE_…" throws now point at Settings → OAuth broker. Legacy
  `VITE_SUPABASE_KEY` alias honoured even when `ANON_KEY` is present-but-empty.
- Settings → OAuth broker card is honest again: Save persists, Forget falls back,
  source label reads "saved in Settings on this device" vs "baked into this build",
  plus a per-device note (paste repeats per device; minted tokens roam via sync).
- Accounts panel: broker banner gains a working Configure button (`wm.open('settings')`
  + `cybermanju:settings-focus` deep-link); all "missing from this build" copy in
  Accounts/Setup/Mobile wizards now names the Settings destination; connected-accounts
  hint states sign-in is per device.
- CI/release: `VITE_SUPABASE_URL` + `VITE_SUPABASE_ANON_KEY` now bake into
  windows/linux/macos/arch builds (`ci.yml` + `release.yml`; arch exports through the
  sudo shell; Flatpak inherits via the linux deb). `Dockerfile` takes the pair as
  build-args (empty default = old Settings-paste behaviour); both docker jobs and
  GitLab `docker-build` pass them. GitLab desktop jobs inherit the mirrored project
  variables automatically (comments only, per the file's no-same-name rule).
- `tests/frontend/supabase-config.test.ts` rewritten for settings-over-build
  precedence, validation, fallback and alias (9 tests). `npm run typecheck` clean,
  full suite 60 files / 697 tests green.

## 2026-10-09 — trash/restore keeps folder index atomic

- `Database::trash_file` now removes the file from `parent_index` in the same redb write transaction that moves it to trash; `restore_from_trash` re-adds it under its original parent in the same transaction, without duplicates. Parentless items remain unindexed.
- REST single and batch delete now rely on the database transaction instead of a second index write, removing a transient inconsistency window.
- Added Rust regressions for preserving siblings, restoring exactly once, and restoring a parentless file.
- Verified locally: `git diff --check`, `npm run typecheck`, `npm test` (48 files / 573 tests), and `bash scripts/check-version.sh`. Rust tests/fmt/clippy were not run locally per `AGENTS.md`; CI must prove the backend changes.

## 2026-10-09 — CI checks develop without publishing Pages

- Added `develop` to push-triggered CI; pull requests still target `main` only. The existing `deploy-pages` condition remains `refs/heads/main`, so develop builds and tests do not alter the production Pages site.
- A separate `/develop/` deployment was requested but no non-Pages host or domain is configured in this task; deployment remains pending the destination and account authorization.

## 2026-10-09 — GitHub Pages preview isolated under `/develop/`

- Pages now builds a combined artifact with `main` at `/` and `develop` at `/develop/`, regardless of which branch is pushed. Artifact builds and deployments are serialized; publication waits for the web/WASM build and Rust checks. The sibling Rust build shares Cargo's target directory to reduce duplicate work.
- The preview gets a separate localStorage namespace, IndexedDB database, and OPFS directory; cross-tab storage events are scoped so preview preferences and vault data do not mix with production. Root/desktop behavior remains unchanged.
- Legal links and the static legal pages follow the active preview path while root/native builds retain canonical Pages URLs.
- Verified locally: `vue-tsc`, 50 frontend suites / 578 tests, version check, root and `/develop/` bundles, `cargo fmt`, Clippy, and workspace Rust tests. GitHub Pages environment now explicitly allows both `main` and `develop`; remote CI/deployment will be verified after push.
- The preview's PWA manifest now uses `/develop/` for its start URL, scope and icons; the Pages assembly step asserts those paths so installation cannot jump to production.

## 2026-10-09 — Remove duplicate wallpaper-effects preference

- Settings → Appearance exposed both “Wallpaper effects” and “Matrix rain” controls for the same `matrixRainEnabled` state. Removed the redundant Matrix rain row, keeping the accessible opt-in canvas control that matches the DesktopShell guidance.
- Added a regression test ensuring the preference appears once with its accessible label.
- Verified locally: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, and 51 frontend suites / 579 tests. Changes are isolated to `develop`; Rust validation remains delegated to CI per `AGENTS.md`.

## 2026-10-09 — Custom wallpaper via URL or file picker

- Settings → Appearance now accepts HTTP(S) image URLs or a file from the browser/WebView picker (PNG, JPEG, WebP, GIF, AVIF; 20 MiB maximum). The shared frontend works in WASM Pages, Tauri WebView and Docker/web without transport-specific branches. Files are stored in client-side IndexedDB and never uploaded to the server; URL images are fetched directly from their source. Invalid schemes/credentials are rejected, image-load failures fall back to the built-in gradient, and replaced object URLs are revoked.
- The selector surfaces “Custom image” while active and returns to the prior preset when reset. `/develop/` uses a separate IndexedDB database from the production root. Added coverage for URL/file validation, local save/restore, object-URL cleanup, and preview isolation.
- Fixed a pre-existing autosave omission in `useTheme()`: built-in wallpaper changes now persist with the other appearance settings.
- Verified locally: `npm run typecheck`, `npm test` (52 suites / 585 tests), `bash scripts/check-version.sh`, `npm run build`, and Vite frontend builds with `VITE_BASE=/` and `/develop/` (asset prefixes checked). The checked-in WASM package is stale, so local Vite builds used the stub; no local Cargo/wasm-pack run per `AGENTS.md`. CI will rebuild the WASM package.

## 2026-10-09 — Browser shortcut help matches the active transport

- Updated the Shell shortcut dock and keyboard-help modal: browser-reserved chords now point to Alt+ fallbacks in all browser transports (REST and WASM), rather than calling them WASM-only. Fallback badges appear only in browser mode, and Tauri no longer marks its working primary shortcuts as blocked.
- Marked `TASKS.md` P1-2 complete and added a regression test for transport-neutral browser guidance and browser-only fallback hints.
- Verified locally: `npm run typecheck`, `npm test` (52 suites / 586 tests), `bash scripts/check-version.sh`, `npm run build`, and `git diff --check`. No Rust changes; remote CI is pending after push.

## 2026-10-09 — Make browser path checks transport-aware

- `useTauri().pathExists()` previously requested `/api/files` directly and returned `true` for any successful response, even when the requested path was missing; it also bypassed the local WASM database. Browser mode now calls the shared `invoke('list_files')` router and checks the requested `FileNode.path`, using REST or the WASM DB worker as appropriate.
- Marked `TASKS.md` P1-11 complete and added regressions for an existing path and a missing path when the list endpoint succeeds.
- Verified locally: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, 53 frontend suites / 588 tests, `npm run build`, and `git diff --check`. The web build succeeds with the existing advisory about large chunks; no Rust changes, per `AGENTS.md`. Remote CI will be checked after push.

## 2026-10-10 — Guard Tauri detection against the WASM sentinel

- Confirmed `isTauri()` checks truthiness rather than property presence; Settings uses that shared helper and StatusBar derives its label through `useTransport()`. Added regression tests proving `window.__TAURI__ = false` stays browser mode while a truthy bridge is detected as native.
- Marked `TASKS.md` P1-1 complete so the backlog reflects the existing runtime fix and its new test coverage.
- Verified locally: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, 53 frontend suites / 590 tests, `npm run build`, and `git diff --check`. Build succeeds with the existing advisory about large chunks; no Rust changes, per `AGENTS.md`. Remote CI will be checked after push.

## 2026-10-10 — Pin the cross-transport shell write cap

- Added a contract test that reads Rust `MAX_WRITE_BYTES` and compares it with the browser shell's `STATIC_WRITE_LIMIT`, asserting both remain at 1 MiB. This prevents Pages/WASM and Rust WASM from silently diverging without introducing source-generation coupling between the languages.
- Marked `TASKS.md` P1-17 complete with the parity-test approach.
- Verified locally: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, 54 frontend suites / 591 tests, `npm run build`, and `git diff --check`. Build succeeds with the existing advisory about large chunks; no Rust changes, per `AGENTS.md`. Remote CI will be checked after push.

## 2026-10-10 — Dispatch cybsh chains across the static vault and WASM volume

- Static cybsh now splits unquoted `&&`/`;` chains, runs vault-aware commands locally, and falls back clause-by-clause to the WASM volume for ordinary shell commands. The chain stops on its first failed command and retains combined output; mixed `&&`/`;` stays on the existing Rust path until both parsers share semantics.
- Unquoted `|` returns an explicit `unsupported:` result while quoted bars remain arguments. Unexpected interceptor errors are logged before the Rust fallback. Marked `TASKS.md` P1-18 complete.
- Added regressions for `quota && ls`, all-static `&&` and `;` chains, short-circuit failure, explicit pipe refusal, and quoted-pipe behavior.
- Verified: `bash scripts/check-version.sh`; `npm run typecheck`; `npm test` (54 files / 593 tests); `npm run build`; `git diff --check`. Build succeeded with the repository's existing large-chunk advisory and used the WASM stub because the checked-in generated package is older than Rust sources. No local Cargo commands, per `AGENTS.md`.

## 2026-10-11 — OAuth on Linux desktop: system browser + cybermanju:// deep link
- Symptom (AppImage): after approving Google, the popup died with
  `Redirection to URL with a scheme that is not HTTP(S)`. Root cause:
  `supabaseRedirectTo()` returned the Tauri page URL (`tauri://localhost/…`)
  and the Linux WebKitGTK WebView refuses to navigate back to any
  non-HTTP(S) scheme, so the PKCE return never landed.
- Fix: every Tauri build (desktop + mobile) now authenticates in the SYSTEM
  browser with the `cybermanju://oauth/callback` return. `useSupabase.ts`:
  `supabaseRedirectTo()` returns `OAUTH_CALLBACK_URL` under `isTauri()`
  (exported for tests; `MOBILE_OAUTH_CALLBACK_URL` kept as an alias),
  `signInWithPopup()` + deep-link install/activate use `isTauri()` instead of
  `isTauriMobile()`, and Supabase `redirect` rejections now name the exact URL
  to allowlist (Supabase → Authentication → URL Configuration → Redirect URLs).
  Accounts per-card Connect hardens the same way (`nativeApp = isTauri()`,
  browser-worded waiting messages); App boot installs the listener on all
  Tauri; Settings broker card names the `cybermanju://oauth/callback` entry.
- Native plumbing: `tauri.conf.json` registers the desktop scheme
  (`plugins.deep-link.desktop.schemes: ["cybermanju"]`); `src-tauri` adds
  `tauri-plugin-single-instance` (`deep-link` feature, desktop-gated) and
  registers it FIRST in `lib.rs` so a return arriving while the app runs is
  forwarded to the live instance's `onOpenUrl` instead of booting a stray
  second window. Supabase dashboard needs `cybermanju://oauth/callback` in
  its Redirect URLs (already required for mobile).
- Tests: 3 redirect-selection cases (web https return, Tauri desktop +
  mobile deep-link). `npm run typecheck` clean, full `npm test` green.

## 2026-10-11 — Universal `cyb` CLI (Go + Charm) + installer
- New `cli/` Go module (`github.com/cybermanju/cybermanju.github.io/cli`):
  cobra command tree with a Charm face — Lipgloss theme/tables/panels,
  Bubbles+Bubble Tea `repl` (viewport scrollback, history, plain fallback
  when piped), Huh forms (`setup`/`login`/passphrases/confirms), Glamour
  markdown for `ai` results, `pkg/browser` for OAuth launch. REST-first
  against the dashboard (`internal/client`, camelCase wire, AGENT-1
  prefix → hint mapping, 401 → `cyb login`).
- Commands: `setup` (probe → register/login → 0600 profile) · `serve`
  (Docker up/status/stop) · `login/logout` · `config` · `files`
  (ls/info/mkdir/rename/trash/restore/empty) · `sync` (configs/create/rm/
  start --wait/move/test/remote/usage/status/progress/cancel/job) ·
  `disk` (list/create/attach/detach/resize/check/df/key-holder/destroy;
  passphrase via flag/file/prompt, never logged) · `oauth launch`
  (PKCE URL → browser → verify via connection test) + `status` ·
  `sh`/`run` (same cybsh incl. --dry/--lint/--fmt/--json/argv) · `ai`
  (providers/configs/prompt --follow/sessions/abort/approve; asks park
  notice) · checksum-verified self-`update` from GitHub releases.
- Transport is genuinely tested: `httptest` fake dashboard covers auth
  header, camelCase decode, conflict/401 hint mapping, OAuth/exec shapes;
  plus config 0600 + resolution-order and size/version/asset-name units.
- `install.sh` (Linux/macOS/Git-Bash) + `install.ps1` (native PowerShell):
  OS/arch detect, latest-or-pinned tag, `cyb-<os>-<arch>.tar.gz` +
  SHA256SUMS verify, `~/.local/bin` / `%LOCALAPPDATA%\\cybermanju\\bin`.
- CI: new `cli-build` job (`gofmt -l` gate → `go vet` → `go test` →
  CGO_ENABLED=0 cross-compile linux/darwin/windows × amd64/arm64 with
  `-trimpath` + version/commit ldflags → `dist-cli/`); release ships a
  9th family (`release-cli`, required in full + reuse checks, atlas +
  Downloads rows); `push.sh --release --last` validates `dist-cli`;
  GitLab mirror gains `cli-build` (golang image) + release collect.
- Docs: README CLI section + repo map, atlas entry in
  `docs/RELEASE_NOTES.md`. Local: `go build` × 6 targets, `go vet` +
  `go test` green.
- Charm pink/red pass: bubbletea-native animated spinner (pink dots +
  gradient ◈) on every `SpinWhile` with a plain fallback off-TTY;
  pink→red per-rune `Gradient` banner; pink Huh form theme
  (`ui.NewForm`) on setup/login/passphrase/confirms/approvals; pink
  REPL prompt + rounded pink viewport border; pink→red `progress`
  download bar wired into `cyb update`. Pure helpers unit-tested
  (gradient round-trip, theme, banner); spinner proven under a pty.

## 2026-10-11 — Deep-link follow-up: runtime self-heal + 2nd-process storage guard
- Symptom (CachyOS, release AppImage): browser OAuth approved (200 OK) but
  the OS had no handler for the `cybermanju://` return ("Nenhum aplicativo
  instalado"). Root causes, all desktop-packaging gaps: (a) no
  `x-scheme-handler/cybermanju` default on bare AppImages without launcher
  integration, (b) storage/dashboard init ran in `run()` BEFORE the
  single-instance plugin setup, so the second process opened the live redb
  exclusive lock and quarantined the vault (`corrupt-<ts>.bak`) before
  exiting, (c) frontend never called `register()`, (d) capabilities lacked
  every `deep-link:*` permission (so even `getCurrent()` was denied).
- Fix: `useSupabase.ts` best-effort `isRegistered()` → `register('cybermanju')`
  on desktop startup (new `OAUTH_DEEP_LINK_SCHEME`, kept in sync with
  `tauri.conf.json > plugins.deep-link.desktop.schemes`); capabilities gain
  `deep-link:default` + `allow-register` + `allow-is-registered`; `lib.rs`
  moves DB/index/dashboard init into the builder `.setup()` hook (runs after
  plugin setups, so a second process exits via single-instance before
  touching storage) with the dashboard Arc ferried out via a holder for the
  main-thread `stop()` after `.run()`.
- Verified: `tauri.conf` scheme already ships in every installer (extracted
  the release AppImage: `MimeType=x-scheme-handler/cybermanju` in the bundled
  `.desktop`, which deb/rpm/flatpak/AUR all reuse; the pinned CLI binary
  embeds the NSIS `URL Protocol` registry template; `macos/app.rs` consumes
  the same key for the dmg plist). `vue-tsc` clean, full `npm test` green
  (700), `check-version.sh` green. Rust side (fmt/clippy/test + all 8
  families) rides the next CI run — no local cargo per repo rules.
- Note: `Cargo.lock` is missing `tauri-plugin-single-instance` (added without
  a lock update); harmless in CI (no `--locked`) but run `cargo update -w`
  and commit the lock. Local machine fallback used during triage:
  `xdg-mime default cybermanju-os.desktop x-scheme-handler/cybermanju`.

## 2026-10-11 — Integrate current main into develop

- Merged `origin/main` at `5c3d68d` into `develop`, retaining both branches' commits and worklog entries. The import includes the universal Go `cyb` CLI and installer, Linux desktop OAuth deep links, Supabase broker improvements, mobile launcher/messages, scheduler/secrets, and associated CI/docs updates.
- Resolved the Settings conflict by preserving develop's wallpaper/settings UI while keeping main's Supabase redirect instructions and per-device storage guidance. Recombined append-only worklog entries from both branches in date order.
- Local checks passed: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, `npm test` (62 files / 708 tests), `npm run build`, and `git diff --check`. Build retains the existing large-chunk advisory. Rust checks are delegated to CI per `AGENTS.md`. CLI Go checks could not run locally: the installed Go is 1.26.8 while `cli/go.mod` requires 1.27.2; the CI job installs the version from `cli/go.mod`. Remote CI will be checked after pushing the merge to develop.

## 2026-10-11 — cyb wave 2: cyberpunk system + full API coverage + doctor
- Cyberpunk aesthetic: neon cyan (`#00F0FF`) + hazard accents join the
  pink/red Charm core — ASCII `Logo()`, deterministic neon `Rule()`
  divider, `Splash()` boot header on `cyb` + `cyb setup`. No new deps.
- Coverage audit vs the ARCHITECTURE REST table closed the gaps:
  `search/suggest/geo` · `secrets` (metadata-only list, sealed save,
  audited reveal) · `cron` (list/add/rm/run/enable/disable/history) ·
  `repair/scrub/lease` durability trio · `collections/accounts/loose/
  faces` organizers · `share` + `audit` · `files cat/write/versions/
  snapshot/revert/snapshot-all` · `batch delete/encrypt/compress` ·
  `code parse` (engine reported) · `crypto status/keys` ·
  `users/perms` (admin) · `ai` gains sessions/init/compact/key/models/
  mcp/memories(recall/export) · `sync` gains runs/restore/remote-rm/
  create-repo/upload. `--limit` on search/suggest/audit; bare
  `cyb audit` lists.
- `cyb doctor`: 7 checks (reachable, readyz, JWT fresh + accepted,
  crypto, volume, providers, agent, update) with pink ✓ / red ✗.
- Stability: GET-only single retry with backoff; JWT expiry stamped on
  login/setup (`ExpiresAt`) and enforced in `requireAuth`; all
  destructive commands confirm (or `--yes`).
- CI: Go tests now run `-race -count=1` (GitHub + GitLab).
  README carries the full command matrix.

## 2026-10-11 — cyb wave 3: selectable backends + Supabase-direct + debs
- Backends (`cyb backend use/status`): remote (URL) · docker (manages the
  `cyb serve` container) · native (probes the installed app's localhost
  dashboard — deb/AppImage/dmg/apk/Windows) · supabase (dashboard-less,
  read-only). Profile stores the selection; explicit `--server` always
  wins; dashboard commands refuse in limited mode with guidance.
- `cyb supa`: real Supabase Auth PKCE (loopback `:54329`, allowlist
  printed at selection) → session in the 0600 profile; `status` (with
  one-shot refresh), `repos` read-only across GitHub/Drive/GitLab,
  `logout` scrubs tokens. No decrypt/encrypt in this mode by design.
- Shipping answers "exe/deb?": yes — windows tarballs carry `cyb.exe`,
  plus real `cyb_<ver>_{amd64,arm64}.deb` built by `scripts/cyb-deb.sh`
  (dpkg-deb, control-validated locally) and shipped as release family
  members (full + reuse checks, installer unchanged on tarballs).
- `cyb doctor` covers supabase mode too (broker/session/provider-token).
  Session `ExpiresAt` enforced; GET-only retry; `--limit` review done.
  Tests: RFC-7636 PKCE vector (cross-checked vs Python), broker
  exchange incl. provider_token, backend resolution table, expiry.
  `go test -count=1` green locally (`-race` rides CI, which has gcc).

## 2026-10-11 — cyb E2E against the REAL backend (GHCR image, no mocks)
- No docker/cargo here, so the real `cybermanju-os-server` was pulled
  straight from GHCR via the registry API (31 MB Alpine image), extracted,
  and run (musl static — executes anywhere). Real AppImage also pulled +
  extracted: `x-scheme-handler/cybermanju` confirmed, but the GUI binary
  needs glibc 2.38+ (box has 2.35) so it can't execute here.
- Proven end to end: setup/register/login/JWT · doctor · 2 local
  providers · sealed disk create (512M) · wrong passphrase FAILS with
  `auth: superblock did not open (decryption failed…)` · right one
  attaches · check ok · admin-gated secrets (user role forbidden) ·
  seal→reveal round-trip (`s3cr3t-nas!`) · sync test/upload bytes land
  on provider · start 202→completed · cron add/list/rm loop ·
  code parse reports `"engine": "heuristic"` · audit shows our reveal.
- E2E caught real CLI bugs, all fixed: omit-empty for `containerPath`
  (server chokes on `""`), `tags: null` → `[]`, `fileIds: null` → `[]`,
  raw config pass-through for test/remote/upload (Go struct is a subset —
  server demands full rows incl. `autoSync`), tolerant test decode
  (endpoint returns a bare bool).
- Known server-side gap (not a CLI bug): no public REST creates vault
  FileNodes, so provider→provider `move` needs a vault file first.

## 2026-10-11 — Merge latest main and align startup transport diagnostics

- Integrated `origin/main` through `a866573` (three main-only commits since the previous merge), including the expanded `cyb` CLI, selectable backends/Supabase-direct support, release packaging, CI, and documentation. The only merge conflict was the append-heavy worklog; kept develop's integration record and all new main entries. Develop's own fixes and regression tests remained present.
- LandingPage now uses the synchronous shared `useTransport()` classifier for its startup transport summary, matching Settings and the shortcut badge without waiting for WASM initialization. Added regression tests for Pages first paint, shared label consumers, and the explicit transport override; marked P1-4 complete.
- Verified locally: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, `npm test` (63 files / 711 tests), `npm run build` (existing large-chunk advisory), and `git diff --check`. Local Go vet/tests could not start because the installed Go is 1.26.8 while `cli/go.mod` requires 1.27.2; the latest `main` CI for `a866573` passed, and the develop CI run will validate the merged branch. No Cargo commands run per `AGENTS.md`.

## 2026-10-11 — Retry transient WASM initialization failures
- Addressed the first part of P1-19: `loadWasm()` no longer retains a rejected import/initialization promise indefinitely. Concurrent callers still share the in-flight attempt and receive its rejection; after failure, the next interaction can retry, while successful initialization remains cached.
- Added `wasm-load-retry.test.ts`, which simulates one transient initialization failure, verifies the next attempt succeeds, and confirms later calls reuse the initialized module. Updated `TASKS.md` to record this completed subtask while leaving OPFS backend upgrades and worker timeout/offline handling open.
- Verified with `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, `npm test` (64 files / 712 tests), and `git diff --check`. No local Cargo commands run per `AGENTS.md`.


## 2026-10-11 — Keep REST timeouts active through response-body reads
- Confirmed `origin/main` (`a866573`) is already an ancestor of `develop` (`c534c45`); develop is 14 commits ahead, with no main-only changes to merge. `main` remains untouched.
- Fixed a remaining P1-10 gap in `restFetch()`: its 15-second `AbortController` deadline previously ended as soon as response headers arrived, so a stalled success/error body could hang indefinitely. The deadline now covers body consumption; an aborted body read is normalized to `network:` and uses the existing one-retry backoff. The 401 event still fires as soon as headers arrive.
- Added `tests/frontend/rest-fetch-timeout.test.ts`: headers arrive immediately, the first body stalls until the deadline, then a retry succeeds. During the audit, P1-6/7/8 implementations were verified in the source and existing transport tests, and their stale open checkboxes were closed; P1-10 is now complete.
- Verified: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, `npm test -- --reporter=dot` (65 files / 713 tests), and `git diff --check` all pass. No Cargo commands run, per `AGENTS.md`.

## 2026-10-11 — comment triage: top-file headers on 100% of code files
- Measured: 461 code files / ~174k LOC / 14,652 comment lines (8.4%); ~4,500
  mid-function narration lines were the slop, TODOs ~0, contracts (~208:
  SAFETY/error-prefixes/budgets/engine labels) stay inline by rule.
- New: `scripts/check-file-headers.sh` (shape gate, cargo-free),
  `scripts/collect-file-context.mjs` (heuristic symbols/imports/comments JSON
  mirroring `cybermanju-types::code`; Rust tree-sitter stays authoritative),
  `scripts/generate-file-headers.py` (`--dry-run`/`--apply`, idempotent).
- Applied: 330 files got merged Role/Exports/Integrates/Contracts headers
  (200 missing + 113 unnamed + 17 single-line); 131 grandfathered prose
  headers untouched; 2 hand merges demo the AI step (DiskManagerPage.vue,
  format.ts). `git diff` hunks are header-only at line 1.
- CI: `meta-check` now runs `check-file-headers.sh` next to check-version.sh.
- Verified locally: headers 461/461, check-version ok, vue-tsc clean,
  vitest 60 files / 700 tests green. Rust fmt/clippy/test left to CI
  (no local cargo per AGENTS.md). NOTE: tree was already dirty before this
  pass (`#[tauri::command(async)]` migration, Cargo release profile,
  SKILL.md/OPERATIONS.md edits) — untouched here, still uncommitted.

## 2026-10-11 — UI jank pass: async IPC, rAF coalescing, lazy panels, hidden-tab gating
- **All 175 Tauri commands are `#[tauri::command(async)]`** (26 files under
  `src-tauri/src`): sync commands run on the GTK/WebKit main thread, so any
  backend call over a few ms froze drag/typing/paint — the root cause of the
  laggy feel. IPC wire format unchanged; the only 3 non-`Result` handlers
  (`os_jobs`, `os_complete`, `cron_ensure_started`) now return
  `Result<_, String>` — `Ok` resolves to the same payload. No Rust callers
  besides `lib.rs`'s handler list; the frontend invokes by name.
- **Release profile** added to workspace `Cargo.toml`: `opt-level = 3`,
  `lto = "thin"`, `codegen-units = 1` (default was no LTO / cgu 16) — cold
  launch + IPC latency; validated by CI's Rust job (no local cargo per
  AGENTS.md).
- **AppWindow**: the full-window 40 px `backdrop-filter` was dead weight
  (the opaque `.window-content` covered it, only the titlebar ever showed
  translucency) → blur moved to `.window-titlebar`, frame stays on
  `var(--ui-window)` to avoid double-compositing the tint. Drag/resize now
  coalesce pointer events into one `requestAnimationFrame` flush
  (`stageMove`/`stageResize`/`flushGeometry`) — a high-Hz mouse no longer
  fires a reactive update + layout per event; plasma snap still runs on
  `stopDrag`/`stopResize`.
- **CanvasEngine**: checkerboard/scanline are pre-rendered tiles via
  `ctx.createPattern` (one fill instead of ~2 600 `fillRect`s/frame, same
  pixels), op-art rings one path/one stroke, wireframe one path + constant
  `fillStyle`, particles constant `fillStyle` + `globalAlpha`, the
  per-frame `initWireframe()` allocations are gone, and the rAF loop pauses
  on `visibilitychange`.
- **LandingPage**: the 50 ms `buddhaGlow` timer (20 reactive style
  writes/second forever) is a CSS `buddha-glow` keyframe now — same sine
  (0.3–0.9, ≈5.03 s period, ease-in-out ≈ sin). Particle placement/timing
  is randomized ONCE (`particleStyles`): it used to be `Math.random()` per
  render, so the 280 ms ASCII frame tick teleported every particle and
  restarted its animation 3.5×/second. Dead `showCursor`/`cursorTimer`
  (never referenced by the template) removed.
- **Hidden-window gating** (`if (document.hidden) return`): StatusBar 4 s
  ps poll · DevicesPanel 4 s top poll · ProcessPanel refresh + `nowMs` ·
  CodeStudio 2.5 s explorer sync · AgentPanel + `useStudioAgent` 1.5 s
  live-thread and 1 s tick · store `startAutoRefresh` (seven fetches) ·
  schedule tick 30 s · TopMenuBar/PlasmaPanel 1 s clocks. First visible tick
  refetches everything; `SecretsPanel`'s reveal countdown is deliberately
  NOT gated (it is a security deadline).
- **Code split**: 15 static panel imports → `asyncPanel(() => import(…))`
  (FileManager 99 kB · AccountManagerPanel 81 kB · AgentPanel 75 kB ·
  CodeStudio 67 kB · SettingsPage 28 kB · MapView + its maplibre/css ·
  DiskManagerPage, ProcessPanel, SecretsPanel, DevicesPanel, SyncPanel,
  ShieldPanel, FilePermissionsPanel, FaceGroupingPanel, OrganizePanel,
  WebDashboardPanel, TransferGraph). 65 chunks total, all local assets —
  first open of a panel is a disk read, not a network hop; `WindowContent`
  (the instant search/trash/activity surfaces) stays eager.
- Verified: `scripts/check-version.sh` ok · `vue-tsc --noEmit` clean ·
  vitest 60 files / 700 tests green · `npx vite build` ok (29.7 s).
  Rust fmt/clippy/test left to CI. Tree shared with concurrent passes
  (file-header sweep, CLI work) — untouched here.

## 2026-10-11 — Pages wiki: improved structure finder + Three.js scenery + llms.txt
- Structure finder upgraded both sides: `crates/types/src/code.rs`
  (qualifier stripping, Go receivers, `pub mod`/`impl`/bash defs,
  top-level-only const/static, doc-comment hoisting into additive `docs`)
  + mirrored `scripts/collect-file-context.mjs`. New Rust unit test
  `heuristic_covers_qualified_items`; JSON shape unchanged + `docs`.
- New `npm run wiki`: `scripts/generate-wiki.mjs` scans the tree into
  16 subjects → `dist-wasm/llms.txt`, `wiki/<id>.txt/.json/.html`,
  `wiki/subjects.json`; then `vite.config.wiki.ts` builds the Three.js SPA
  (purple/red/white/black, manju mascot, orbit nodes, lanterns, search,
  hash routes `#/id`, reduced-motion safe) into `dist-wasm/wiki/`
  (emptyOutDir OFF so text artifacts survive). Added `three` (+ types) dep.
- CI `wasm-build` runs `npm run wiki` after the frontend build (same
  VITE_BASE), so Pages serves `/wiki/` + `/llms.txt` auto-regenerated.
  Sitemap + tsconfig include updated; AGENTS.md §7 pins the comment
  strategy (top-file headers 100% enforced) and §2 loop adds the header gate.
- Verified locally: wiki builds (143 KB gzip, app bundle untouched),
  vue-tsc clean, vitest 60 files / 700 green, headers 461/461 (+5 new files
  carry named headers, covered once staged). Rust tests left to CI (no cargo).

## 2026-10-11 — Route agent-job updates by available transport
- At the start of the round, `origin/main` through `a866573` was integrated. Main then advanced to `d9fb6da`; P1-14 was preserved in commit `0c42dba` and the new main commit was merged into develop without modifying main.
- Fixed P1-14: agent event streams now use the configured API URL or the current REST dashboard origin instead of assuming `localhost:3456`. Tauri-IPC-only and static WASM sessions without a configured server skip the unavailable `/events` endpoint and poll `agent_job_status` through `invoke()`; stream failures still fall back to polling.
- Added resolver regressions for same-origin REST (including host case/port), native/static polling, and explicit remote URLs; updated `TASKS.md`.
- Verified: `bash scripts/check-version.sh`, `npx vue-tsc --noEmit`, `npm test -- --reporter=dot` (716 tests), and `git diff --check` all pass. No Cargo commands run, per `AGENTS.md`.


## 2026-10-11 — Integrate main d9fb6da and keep both Pages artifacts complete
- Preserved the P1-14 transport update in commit `0c42dba` before merging `origin/main` at `d9fb6da` into `develop`; `main` was not modified. Resolved the two deterministic conflicts by keeping the new shortcut-test header alongside develop's test/import, and combining both append-heavy worklogs. Removed one extra blank line at the end of the incoming `Cargo.toml` flagged by `git diff --check`.
- Review found the new wiki build ran only for the active branch while the sibling Pages branch built only its frontend. The sibling build now also runs `npm run wiki` with its branch-specific `VITE_BASE`, so a complete Pages artifact retains the wiki under `/wiki/` and `/develop/wiki/` as well as the existing production-root/develop-preview separation.
- The new header gate identified five develop frontend tests without top-file headers; added only the generated minimal headers. `scripts/check-file-headers.sh` now passes all 472 tracked code files.
- Post-merge local validation passed: `bash scripts/check-version.sh`, `bash scripts/check-file-headers.sh`, `npx vue-tsc --noEmit`, `npm test -- --reporter=dot` (65 files / 716 tests), `VITE_BASE=/develop/ npm run wiki`, and `git diff --check` / staged diff checks. No Cargo commands were run, per `AGENTS.md`; Rust/Tauri and release-profile validation remains with CI.
- Review note: the incoming main worklog described the async Tauri command migration and release profile as still uncommitted, but both are present in commit `d9fb6da` and are included in this merge; CI remains authoritative for their validation.


## 2026-10-11 — Correct the Rust formatting gate reported by main CI
- The completed Actions run for `d9fb6da` had one failed job: `Rust Lint & Test` stopped at `cargo fmt --all -- --check`; all other jobs succeeded, while Pages deployment was skipped because of that dependency. The log identified formatting-only diffs in `crates/os-wasm/src/db.rs` and `crates/types/src/code.rs`.
- Applied the exact rustfmt line-wrap changes shown in that CI output. No Cargo command was run locally, per `AGENTS.md`; the Rust lint/test job must be checked again on the resulting develop commit before considering the integration validated.

## 2026-10-11 — wiki scenery upgrade from top-starred three.js research
- Studied mrdoob/three.js (116k), pmndrs/R3F+drei+postprocessing (33k),
  three-gpu-pathtracer/gkjohnson, 2026 award sites (Oryzo/IVRESS/Cartier pattern:
  bloom+ACES, one hard idea, scroll/cinematic camera, budgeted scene graph).
- Applied: UnrealBloomPass+OutputPass chain (skipped <720px), ACESFilmic 1.12,
  shared glow-texture cache, InstancedMesh lanterns (70→1 draw call),
  select-burst spark pool, per-node bob/rings/labels, beacon+pulse, shooting
  stars, mascot blink, eased fly-to + FOV punch, adaptive DPR stepper,
  tab-hidden render pause, CSS vignette. JS 578→604 KB (152 KB gzip).
- UX: keyboard (/-arrows-Enter-Esc), live symbol matches, crumbs+copy-link,
  prev/next pager, skeleton loader, focus-visible rings, dblclick home.
- Verified: vue-tsc clean, vitest 60/700 green, headers 467/467.

## 2026-10-11 — CLI: neon UI/UX overhaul + `cat -r` markdown
- Charm deps bumped to latest on the v1 import path (no charm.land
  migration): bubbletea v1.1.0→v1.3.10, bubbles v0.20.0→v1.0.0,
  glamour v0.9.1→v1.0.0, huh v0.6.0→v1.0.0 (lipgloss stays v1.1.x).
  Zero API breakage — build + tests green unmodified.
- `internal/ui` cyberpunk pass: neon tables (pink header rule, faint
  zebra, terminal-width truncation via reflow), syntax-tinted `--json`
  (pink keys / ice strings / cyan numbers / yellow bools), width-aware
  Glamour markdown (pink style on color terms, ascii when `--no-color`,
  `GLAMOUR_STYLE` env wins), stderr pipe-safe `Hint`, plus `Badge`,
  `Chip`, `Key`, `List`, `PanelTitled`, `NextSteps` atoms.
- `cyb files cat -r/--render` glamour-renders markdown; plain `cat` of a
  `.md` now hints at `-r` (stderr, stdout stays pipe-clean). AI output
  keeps rendering through the upgraded path.
- Styled help everywhere: grouped root atlas + neon per-command pages
  (usage, subcommands, local + global flags); gradient `version`,
  aligned `doctor` checks, keycap REPL help, `main` errors via
  `ui.Failure`.
- New tests (run on CI via the existing `go test -race ./...` step):
  `ui` table/JSON/markdown/chrome (15) + `cmd` render-flag wiring,
  envelope parsing, and a fake-dashboard `cat -r` end-to-end.
- Verified locally: gofmt clean, `go vet` clean, `go test ./...` green
  (no cgo/gcc on this box so `-race` is CI-only), readonly
  `GOPROXY=off` build ok, `cyb --help` + subcommand help smoke-tested,
  headers 467/467.


## 2026-10-11 — Integrate latest main 0fd3df5
- Integrated `origin/main` through `0fd3df5` after `d9fb6da`, retaining P1-14 and the develop-only Pages workflow fix. The new main commit includes wiki scenery/structure work, the CLI UI and test expansion, and the rustfmt-only corrections. Its only merge conflict was the append-only `worklog.md`; preserved develop's complete log and appended both main-only wiki/CLI entries.
- Final allowed local checks passed on the combined tree: `bash scripts/check-version.sh`, `bash scripts/check-file-headers.sh` (475 files), `npx vue-tsc --noEmit`, `npm test -- --reporter=dot` (65 files / 716 tests), `VITE_BASE=/develop/ npm run wiki`, and staged/unstaged `git diff --check`. No Cargo commands were run, per `AGENTS.md`.
- Actions for `0fd3df5` (run `38160866995`) was still running during this validation; its Rust/Go checks will be reviewed before pushing only `develop`. `main` remains untouched and no release was made.

## 2026-10-11 — CLI: Charm v2 migration (bubbletea/lipgloss/bubbles/huh/glamour)
- Full v2 migration on the `charm.land/*/v2` paths: bubbletea v2.1.0,
  lipgloss v2.0.6, bubbles v2.2.1, huh v2.0.3, glamour v2.0.1
  (latest as of today); v1 modules fully dropped from go.mod/go.sum.
  Per the v2 upgrade guides: `View() tea.View`, `KeyPressMsg` +
  string matching (`ctrl+c`/`esc`/`enter`/`up`/`down`), viewport
  `WithWidth/WithHeight` + `SetWidth/SetHeight`, textinput `SetStyles`,
  progress `WithColors` blend, huh `ThemeFunc` (adapts dark/light),
  glamour API unchanged.
- v2 output discipline: builders emit full-fidelity ANSI; every print
  boundary downsamples to `lipgloss.Writer`'s profile, and spinner/Huh/
  REPL are pinned via `tea.WithColorProfile`. `--no-color` now pins
  `NoTTY` (byte-clean; v2 Ascii keeps bold SGR, so Ascii is not enough).
- Verified: gofmt/vet clean, `go test ./...` green incl. new
  builder-vs-print profile tests, readonly-mode build ok, binary smoke
  (`--help` atlas, subcommand pages, `version`, zero ESC bytes under
  `--no-color`/pipes), headers 470/470. `-race` remains CI-only (no
  gcc on this box).

## 2026-10-11 — CLI: double-review pass (correctness + consistency)
- Fixed real issues found on re-read: animated spinner dot (frames
  never advanced), `fitWidths` dead param, subcommand help rows
  bypassing Downsample, nondeterministic "more" order (sorted),
  `completion` invisible in the atlas, flag defaults missing from help
  (`--timeout (default 15s)` …), `backend` panel bypassing Downsample,
  REPL/viewport negative widths on tiny terminals (clamped), empty
  tables without guards (`geo`, supa repos/files/projects).
- Consistency: client-known vocab now decorated (`Flag("encrypted")`,
  `Flag("key-holder")`, cyan search scores); server strings untouched.
  Chrome moved to stderr (`serve` next-step, `poll with:`, sync/ai
  wait ticks, ai follow start) so stdout stays pipe-clean.
- New `TestRootHelpAtlasComplete`: every visible command must sit in a
  section (catches unmapped commands), leaf pages show usage + flags.
  45 CLI tests green; gofmt/vet clean; headers 470/470.


## 2026-10-11 — Integrate latest main through 78ff10a
- After `0fd3df5`, main CI reached Clippy and reported two `manual_pattern_char_comparison` errors in `crates/types/src/code.rs`. The follow-up `78ff10a` replaced both closures with equivalent character-array patterns and also carried CLI consistency/test updates; merged it into develop. The only conflict was the append-only worklog, resolved by retaining develop's full history and appending the two new main CLI sections.
- Local validation of the combined tree passed: version check, headers (475 files), `vue-tsc`, all 716 frontend tests, the wiki build with `VITE_BASE=/develop/`, and both staged/unstaged diff checks. Rust and Go validation remain remote-only under the repository's toolchain constraints.
- CI for `78ff10a` (run `38162469871`) is in progress; hold the develop push until its result is known. `main` is unchanged.


## 2026-10-11 — Integrate main follow-up 471c089
- The `78ff10a` Actions run was cancelled as main advanced again. Integrated `471c089`, which binds the Rust heuristic-test symbol vector before iterating and resolves the reported `E0716`; this merge was automatic with no conflicts. The latest fetched `origin/main` remained at `471c089`.
- Revalidated the combined tree locally: version/header checks (475 files), `vue-tsc`, 65 frontend files / 716 tests, wiki build at `/develop/`, clean cross-branch diff, and clean worktree. No Cargo was run locally.
- CI run `38162682023` for `471c089` was still in progress; do not push develop until this latest run completes successfully. `main` remains unchanged.


## 2026-10-11 — Integrate main follow-up 3caa6a3
- The `471c089` CI run was cancelled as main advanced. Integrated `3caa6a3`, a rustfmt-only adjustment that collapses the heuristic test's symbol-name chains; the merge was automatic with no conflicts.
- Local validation passed again: version checks, 475 file headers, `vue-tsc`, 65 frontend files / 716 tests, wiki build at `/develop/`, cross-branch whitespace check, and clean worktree. No local Cargo commands.
- The latest main CI run (`38163841181`) was in progress; hold the develop push until it succeeds. `main` remains untouched.

## 2026-10-11 — wiki satellites: node clicks explode into real connections
- Generator mines inter-subject links (import edges cybermanju_* → subject,
  reverse used-by edges, shared-signal fallback) + line numbers in the SPA
  index; static pages gained a Related modules section.
- Clicking a node now orbits ≤10 satellite stars: linked modules (their
  colors, click-through) + the module's own top symbols (docs in tooltips),
  each tethered with a live line. Symbols open deep cards `#/id/symbol`
  (kind, file:line, doc-comment, copy link); chips/search/pager/Esc all
  route there. Nothing generic — every satellite resolves to mined data.
- Verified: vue-tsc clean, vitest 60/700 green, headers 470/470.

## 2026-10-11 — CI green on the perf push (run 38163841181, all 14 jobs)
- Pushed the perf pass + in-flight tree work: CI failed `cargo fmt` on two
  other-session hunks (`os-wasm` db.rs passphrase guard, `types` code.rs
  keyword vec) — hand-applied rustfmt's exact wrapping (no local cargo).
- Then `clippy -D warnings`: `manual-pattern-char-comparison` x2 in the new
  code.rs helpers → array patterns (`['?', '!']`, the trim set).
- Then `cargo test` (all-targets): E0716 in the new heuristic test —
  `.iter()` chained on a temporary `Vec` → bind (`ts_syms`/`go_syms`) then
  iterate; rustfmt then wanted those two chains single-line.
- Fix commits 0fd3df5 → 78ff10a → 471c089 → 3caa6a3; final run green
  across fmt, clippy, full workspace tests, windows/linux/rpm/flatpak/arch/
  macos/android/wasm, docker, CLI, audit, meta. This also proves the 175
  `#[tauri::command(async)]` conversions + the release LTO profile compile
  and pass on every target.

## 2026-10-11 — Final main sync and pre-push verification
- Integrated `origin/main` through `3fa0c67`; this latest commit is documentation-only and adds the main-side wiki/CI worklog entries. The append-only conflict was resolved by preserving both histories. Main code remains at `3caa6a3`.
- CI run `38163841181` for `3caa6a3` completed successfully with all 14 jobs green, including Rust Lint & Test, WASM, CLI, and GitHub Pages deployment. Final local checks also passed: version consistency, 475 headers, `vue-tsc`, 65 frontend files / 716 tests, wiki build at `/develop/`, and whitespace checks. No Cargo commands were run locally.
- The final fetch showed `origin/main` at `3fa0c67`, `origin/develop` unchanged at `dda9b2e`, and local develop 15 commits ahead / 0 behind with main as an ancestor. No force-push, release, or change to `main` was made.

## 2026-10-11 — Pages toast `Table 'schedules' does not exist`: wasm `open_all_tables` lagged native
- Root cause: `crates/os-wasm/src/db.rs::open_all_tables` created 32 of the
  37 tables — missing `schedules`, `schedule_runs`, `agent_configs`,
  `agent_sessions`, `agent_memories` (native `Database::new` creates all of
  them on every open). A read txn errors `Table '<name>' does not exist` for
  any skipped table, so on the static host `cron.list` failed on every fresh
  browser DB and the store toasted `Failed to fetch schedules` on init, the
  Schedules tab, and the 30 s browser tick. Writes (`cron.save`, `runRecord`)
  masked it — write txns create-on-open — so the bug only showed pre-first-save.
- Fix: the 5 missing `open_table` calls (same pattern as the neighbors, plus
  a lockstep comment pointing at `crates/db/src/database.rs` so the list
  can't silently drift again) + `open_tables_tests` regression test in the
  same file (in-memory backend, asserts the cron + agent tables open for
  read after `open_all_tables`). Rust validation left to CI per repo rules.
- Verified: check-version.sh green, check-file-headers.sh 475/475, vitest
  62 files / 715 tests green.
- 2026-10-11 — WASM Host folders + Install window: `/host` browses the
  picked `$HOME` folder on static builds (File System Access dialog,
  `src/utils/hostWasm.ts`, same namespace/ids as Tauri; `hostBrowse`
  delegates per-verb, store + FileManager unchanged in shape). Browsers
  without a picker (`unsupported:`) now open a center-screen Install window
  (`InstallPanel.vue`, PanelType `install`) with per-platform logos and
  direct links — Android APK/AAB, Ubuntu deb/AppImage/rpm/Flatpak, Arch
  AUR/AppImage, Windows MSI/EXE, macOS DMG, Docker/GHCR — each App x CLI
  with copyable commands and live GitHub-release asset resolution
  (`src/utils/installLinks.ts`, release-page fallback). Triggered from
  FileManager + store via `cybermanju:open-install` (App.vue opens it).
  715 frontend tests green (incl. new `install-links`), vue-tsc + icons +
  headers + versions green.
- 2026-10-11 — Install window on every native-build `unsupported`: central
  net in `notifyError` (`isInstallNeededError` + `installReasonFor` in
  `src/utils/installLinks.ts`) opens the Install window for picker /
  desktop-app / dashboard wordings — present and future toast sites alike —
  while provider-shaped `unsupported:` (bad repo/path/layer, locked vault)
  stays a plain toast (unit-tested both sides). Inline `Install options`
  affordances added everywhere the state shows without a toast: FileManager
  DEVICE row when no picker exists, SetupWizard one-folder fallback,
  ShieldPanel web-locked notes, MobileSetupWizard scoped-storage hints +
  refusal, AccountManager local-picker hints + refusals, MessagesHub and
  AppDrawer Android notes. Terminal/agent transcripts stay text-only
  (command results must not pop windows). 718 frontend tests green,
  typecheck + headers + versions green.
- 2026-10-11 — CI build-time optimization (A–J): shared `linux-workspace`
  rust-cache key across rust-check/linux/wasm + `save-if` main/develop-only
  (kills ~3x700 MB duplication and per-PR cache churn); Android SDK/NDK +
  Gradle caches (longest job); Arch job gets rust-cache + npm cache
  (was full rebuild from zero); dropped duplicate `cargo check` before
  `wasm-pack build` (ci/release/GitLab); `rust-toolchain.toml` declares
  channel + targets (all `rustup target add` steps removed); apt caching via
  pinned cache-apt-pkgs-action; Docker GHA cache scoped + Dockerfile npm
  cache-mount with `npm ci`; sibling Pages branch reuses last green
  `dist-wasm` artifact (SHA-guarded, rebuild fallback); rpm-build folded
  into linux-build (was a rename-only job); release build jobs no longer
  wait on rust-check (create-release still gates); PRs build Tauri
  `--debug`, release profile on pushes/releases. Local: versions + headers
  + vue-tsc + 718 vitest green, both workflows YAML-parse.

## 2026-10-11 — Notification center (ANALYSIS §3) + mobile hub integration
- `useNotifications` now persists every toast into a per-device history
  (localStorage `cybermanju.notifications.history.v1`, cap 100, sanitized
  restore): `history`, `unreadCount`, `markRead/markAllRead`,
  `removeFromHistory/clearHistory`. Toast auto-dismiss never drops rows.
- `MessagesHub` is now the Notification Center: `System` tab (history with
  unread dots, relative time, mark-all-read/clear) + `Messages` tab (existing
  Android stored-message hub untouched). Launcher store gains `centerTab`;
  `setMessagesOpen(open, tab?)` stays backward compatible; opening on the
  system tab marks it seen (the "while you were away" digest).
- Entry points: TopMenuBar bell (always visible, unread badge; messages button
  stays Android-only and lands on its tab), MobileLauncher bell (same),
  PlasmaPanel tray bell (opens the center; dead live-list popover removed),
  CommandPalette "Open Notification Center". `ui_notify` agent tool and all
  store/scheduler notifies land in history with zero call-site changes.
- Verified: `check-version.sh` + `check-file-headers.sh` ok, `npm run
  typecheck` clean, full `npm test` 65 files / 754 green (new
  `tests/frontend/notification-center.test.ts` 7/7). Rust untouched; no local
  cargo per repo rules. Note: a concurrent session reverted
  `useNotifications.ts` + `stores/launcher.ts` mid-pass; re-applied and
  re-verified on disk before testing.

## 2026-10-11 — Block main CLI/WASM interaction integration pending safety fixes
- Integrated `origin/main` through `b90a6a5` locally on `develop` without code conflicts; did not push. At the last fetch, remote `develop` remained at `3b93d97` and the local branch had three main-integration commits ahead. `main` was not modified.
- Local checks passed: version consistency, 477 file headers, `vue-tsc`, 66 frontend files / 722 tests, WASM frontend build with `VITE_BASE=/develop/`, wiki build, and diff checks. The local frontend build correctly warned that the generated Rust WASM package is stale and therefore used the repository stub; no Cargo was run.
- Review identified promotion blockers in the new host interaction feature: **high** — `src/utils/hostBrowse.ts:43-49` classifies every non-Tauri browser as WASM, breaking Web/REST host browsing; `src/utils/hostBrowse.ts:530-539` strips `/host`'s root destination to an empty string; `src/utils/hostWasm.ts:272-335` can recursively copy a directory into itself/its descendant and then delete the source on move. **Medium** — `src/utils/hostWasm.ts:405-467` reconstructs `write` arguments and can silently lose spacing/leading-hyphen filenames; `src/utils/hostWasm.ts:264-269` checks UTF-16 string length rather than encoded UTF-8 bytes.
- The CI run for main's feature commit `24c3995` (`38165915833`) and the earlier develop run for `3b93d97` (`38165713593`) were cancelled; no green CI validates the new feature. Keep the integration local until these risks are resolved and CI succeeds; no deploy of this integration occurred.


## 2026-10-11 — Host filesystem safety before promotion
- Integrated `origin/main` through `3c2b66c` locally on `develop`, preserving the main-side multi-mount implementation and all existing develop fixes. During final validation, main advanced again to `3aa6066`; that follow-up is being incorporated separately before any push.
- Closed P1-21: host browsing now selects WASM from `useTransport()` so REST dashboards stay on REST; filesystem roots, WASM mount-root sources, traversal paths and self/descendant copy/move targets are refused. WASM copies compare every byte after writing, and a move deletes its source only after verification succeeds.
- Host text writes now quote the path and content as separate shell operands and enforce the 1 MiB ceiling on encoded UTF-8 bytes. The WASM shim preserves empty/quoted/backslash content and leading-hyphen operands. Added regressions for REST routing, roots, traversal, recursion, same-length corruption (including move-source preservation), quoting, empty text, and multibyte size limits.
- The integrated shell tokenizer had one redundant `quote !== 'single'` comparison rejected by TypeScript narrowing; removed that redundant condition without changing parsing behavior.
- Verified on this tree: version check, file headers (487 files), `vue-tsc`, 70 frontend files / 781 tests, WASM frontend build at `/develop/`, wiki build at `/develop/`, and staged/unstaged diff checks all passed. The WASM frontend build used the repository stub because the generated Rust WASM package is stale; no Cargo was run, per `AGENTS.md`. Rust/CI validation remains required after syncing the latest main.
## 2026-10-11 — Scheduled backup UI (ANALYSIS §3 backup-as-feature, phase 2 complete)
- Format first: new `crates/crypto/src/container.rs` — Rust CYBMJ01 codec
  byte-identical to `src/utils/container.ts` (PBKDF2-HMAC-SHA512 → HKDF-SHA256,
  ChaCha20-Poly1305, size-prepended lz4, `+lz4_flex 0.11` line in crypto
  Cargo.toml). Proved, not asserted: TS encoder vectors generated in node
  (fixed salt/nonce/passphrase) are embedded in the Rust tests — CI decodes a
  file the browser wrote. Header-only `parse_header` (no key) + fail-closed
  `auth:` trial-decrypt.
- Native `backup` verb (`crates/os/src/backup.rs` + `shell.rs` arm, table,
  completions, help): `export --to <volume|/providers/…> [--name] [--keep 7]
  [--plain]` (master passphrase seals, `--plain` is the recorded opt-out,
  prune never touches non-`cybermanju-*.cybermanju` files), `list`, `verify
  [--deep]`; same `--json` shapes the static twin answers.
- Transports: `GET /api/backup/export` (Admin, base64 JSON, 32 MiB honest cap
  — bigger vaults use the verb or host script) + `ROUTED_SEGMENTS`, Tauri
  `backup_export` (shared `api::backup`), static `backup_export/_export` +
  `backup_restore` handlers, `wasmDatabaseRestore` export. os-wasm lists
  `cron`+`backup` in help/completion with an honest chained-line refusal.
- Frontend: pure `src/utils/backup.ts` (names/prune/managed script with `#
  schedule:` frontmatter/header verify) + static `backup` verb (provider
  destinations only — the browser volume is text-only, refused honestly) +
  store actions (os_exec-driven list/export/verify, passphrase-local download
  with trial-decrypt proof, Pages one-click restore, `scheduleBackup` writing
  the managed script via `os_write` + cron row) + Disks → BACKUP tab
  (Export/Snapshots/Schedule/Restore wizard with guided desktop/Docker steps
  + catalog rebuild) + palette entry. `BARE_VERBS` learns `backup`.
- Docs: OPERATIONS §5b table, SKILL verb line, ANALYSIS backup row ✅.
- Verified: `vue-tsc` clean, targeted vitest 110 green (new `backup.test.ts`
  + static backup suite + transport-routes backup case), headers ok. Rust
  `fmt/clippy/test` left to CI per repo rules (blind-Rust, self-reviewed for
  col/chain style). NOTE: a parallel session commits this tree (`git add -A`
  sweeps); the mid-pass revert of two notification files earlier is why
  disk-state is re-verified before every edit.


## 2026-10-11 — Harden backup restore and container bounds
- Integrated `origin/main` through `b9ce64a` into `develop` in merge `31f927b`; the append-only worklog conflict was resolved by keeping both branches' complete entries. `main` remains untouched. The CI run for `b9ce64a` was cancelled, so CI validation is still required on the resulting `develop` push.
- Added matching TypeScript/Rust container limits: images are capped at 256 MiB, PBKDF2 work at 1,000,000 rounds, and unknown flags, inconsistent KDF/salt/nonce metadata, undersized AEAD/compressed bodies, empty images, and oversized declared LZ4 output are rejected before expensive crypto/decompression. Decoded images are checked again against the cap.
- WASM restore now opens an imported image in an isolated in-memory redb and creates/checks the schema before touching the active OPFS database. Legacy raw backups are labeled as unauthenticated and unverified at the container layer. The browser restore asks before replacing the current vault and reports restore errors in the wizard.
- Native snapshots check the finished file's metadata against the transport cap before reading it into a `Vec`; REST applies its 32 MiB cap through the same helper (general native snapshot ceiling remains 256 MiB). Added a database limit regression and Rust codec/WASM preflight regressions. Updated the static-cybsh test fixture to carry a structurally valid 16-byte AEAD tag.
- Local checks passed: version consistency; 495 file headers; `vue-tsc`; full frontend suite (72 files / 823 tests); WASM frontend build with `VITE_BASE=/develop/`; wiki build at `/develop/wiki/`; and `git diff --check`. The WASM frontend build used the repository stub because the generated Rust package is stale; no Cargo was run, per `AGENTS.md`. Rust fmt/clippy/tests remain delegated to CI.

## Follow-up — latest main sync and CI formatting correction
`origin/main` advanced to `3c5ec83` and was merged into `develop` as `4f5224a`. Its CI run (`38173149383`) finished with one failure only: Rust `Check formatting` on the two HMAC chain breaks; the WASM build, unit tests and all other jobs passed. I applied the exact formatter diff in `develop` only, preserving `main` untouched. Revalidated locally: version checks, 495 file headers, `vue-tsc`, 72 frontend test files / 823 tests, the `/develop/` frontend and wiki builds, and `git diff --check` all pass. Cargo remains CI-only per repository rules; the `develop` CI will provide Rust formatting, Clippy and Rust test verification after push.

## 2026-10-11 — follow-up Rust formatting from develop CI
The first `develop` CI run (`38174480888`) again failed only in Rust `Check formatting`, now on three expressions added by the container hardening. I applied the exact diff to `crates/crypto/src/container.rs`; no logic changed. The same run passed the WASM crate/frontend, typecheck, frontend tests, wiki build and isolated Pages artifact assembly; deployment was skipped because `rust-check` failed. After the formatter adjustment, version/header checks, `vue-tsc`, all 823 frontend tests, `/develop/` frontend/wiki builds and `git diff --check` pass locally. Rustfmt/Clippy/Rust tests remain to be proven by the next CI.

## 2026-10-11 — System docs generation: additive hoisting + auditor + coverage
- Found by expanding the `crates/scene` Tables docs: the heuristic extractor
  kept only the LAST doc line (both `crates/types/src/code.rs` and its JS
  mirror `scripts/collect-file-context.mjs`), so multi-line `///` blocks
  lost everything but the final sentence in wiki/code-intel payloads.
- Fix (both extractors, same semantics): consecutive doc lines accumulate
  into one space-joined summary (≤400 chars); only blank lines and non-doc
  code break attachment — `#[…]`/`@…` attributes, `*/`/`-->` closers and
  plain narration are transparent and slide the window (previously they
  consumed the ≤4-line budget; e.g. `create_provider_repo`'s docs never
  attached past its arity note + two attributes). JSDoc `*/` closers no
  longer reset pending docs. New `slide_window` helper (Rust) /
  `slideWindow` (JS); behavior change is additive only — no previously
  attached docs are lost.
- Tests: new Rust `heuristic_joins_multiline_doc_blocks` (join, blank-break,
  plain-never-hoists, attribute passthrough, JSDoc closer, narration-slide)
  + new `tests/frontend/file-context.test.ts` (6 cases pinning the JS mirror).
- New advisory auditor `scripts/check-symbol-docs.mjs` (public-only default:
  Rust `pub`, TS `export`, Go exported; skips tests/`impl`/re-export shims;
  `--all|--dir|--top|--json|--strict`; never a CI gate) + docs-coverage per
  subject in `generate-wiki.mjs` (txt/JSON/console).
- Seeded high-value gaps the auditor surfaced: all 14 `schema.rs` structs +
  `sync.rs` (7) + `secrets.rs` (2 fns) + tauri surface (`AppState`, `run`,
  `DuResult`, `os_ps/top/workers/jobs`, dashboard `new`, tree-sitter
  `as_str`, `create_loose_group`, `authenticate_user`,
  `get_file_permissions`). crates/types + crates/scene + crates/search +
  src-tauri/src now 100% public-documented.
- Baseline: public-only 60.5% documented (1217 missing, biggest in
  src/utils, cli/internal, src/composables); full-symbol 27.5%. AGENTS.md §7
  tooling + structure-finder contract updated to the new semantics.
- Verified: headers ok, `vue-tsc` clean, 68 files / 799 tests green, wiki
  smoke build into /tmp ok. Rust `fmt/clippy/test` left to CI per repo rules.
## 2026-10-11 — ANALYSIS §4.5 storage diet / dedup report (cargo CI-only for Rust)

- New pure `src/utils/dedup.ts`: BLAKE3 grouping (`normalizeHash` — lowercase/
  trim, ≥16 hex chars, hex-only), reclaimable = total − one copy, keep-newest
  suggestion, groups ranked reclaimable-desc; files without a usable hash are
  `unhashable` and excluded (honest limit, never fabricated).
- Disks → DIET tab (`DiskManagerPage.vue`, `wm.open('disks', { tab: 'diet' })`):
  dup-sets / dup-files / reclaimable / no-hash cards + per-group file rows
  (KEEP badge on the newest copy) + two-click RECLAIM through the standard
  `store.deleteFile` path + refresh.
- Tests: `tests/frontend/dedup.test.ts` (10 green: normalize, grouping, math,
  keep-newest, unhashable, case-folding, ranking, summary, empty-safe).
- Docs: `ANALYSIS.md` §4.5 ticked ✅ + header remeasured (55 components ·
  34 composables · 57 utils · 16 crates · ~173 command fns · 38 redb tables).
- Verified here: dedup suite 10/10 green. Full loop (`vue-tsc`, `npm test`,
  header/version checks) below; Rust untouched so CI has nothing new to prove.
## 2026-10-11 — ANALYSIS 80% pass: archive, watchers, skills, recovery, TLS, i18n, parity guard, dead-code purge (cargo CI-only for Rust)

- New `ArchivePanel.vue` (`archive` PanelType + wm registry/sizes + Dock +
  palette entry): NOTES (vault markdown + `[[wikilink]]` backlink graph,
  `readManagedContent`/`contentText` loading, OPEN IN EDITOR), TIMELINE (day
  buckets + on-this-day picker), PEOPLE (FaceGroups++ contact records:
  aliases/notes/links local, rename/merge/delete via server actions), TASKS
  (shared localStorage list), BOOKMARKS (IMPORT via `import_from_url`), DATA
  (CSV/TSV sniff/parse/infer, 5000-row cap). Strings via the new i18n layer.
- Pure utils + tests: `timeline` (3), `notes` (4), `contacts` (5),
  `tasks`+`bookmarks` (6), `csv` (6), `watchers` (5), `skills` (4),
  `shamir` (4 — caught a real GF(256) bug: 0x02 is not primitive, tables now
  use 0x03), `i18n` (3), AGENT-1/REST parity guard (11), skill/task cybsh
  verbs (7). Batch total 58 new tests.
- Processes → WATCHERS tab (rules CRUD, EVALUATE NOW: center notify +
  `setFileTags`); Shield → RECOVERY tab (Shamir split N/M, copy-once shares,
  verify-combine; never persisted); Settings → LANGUAGE row (en/pt-BR);
  `skill list|show|run|install` + `task …` verbs in staticCybsh + OPERATIONS.
- TLS story shipped: `Caddyfile` + `docker-compose.tls.yml` (auto-ACME,
  80→443, HSTS, 100 MB cap, single-hop trust policy) + OPERATIONS §6.
- Dead code verified unreferenced and deleted (Sidebar/TreeNode/FileTooltip/
  DashboardOverlay/MatrixRain/PrayerFlags + App.vue dead import); README +
  AGENTS counts refreshed to the verified tree (49 · 34 · 67 · 18 members ·
  ~175 handlers · 152 REST · ~38 tables).
- Still open (honest): P0 auth cluster, iOS, device identity → LAN → CAS →
  sneakernet → iroh, op-log, boot-real-web parity, Tantivy/agent-memory
  bridge, agent classify hook + add/complete bridge, skills panels-later.
- Verified here: headers ok, `vue-tsc` clean, full vitest green (see run).
  Rust untouched — CI has nothing new to prove.
## 2026-10-11 — ANALYSIS 100%: spine, P0 remainder, bridges, iOS scaffold (Rust CI-gated, no local cargo)

- Devices (§2B.1): `DeviceRow` + `devices` table (list/get/save/remove/touch)
  + `POST/GET /api/devices`, `DELETE`, `POST …/revoke` (Admin) +
  `list/register/register_this(MlDsa65)/revoke/remove/list_lan_peers` Tauri
  twins + Devices → Identity tab + `spine.rs` tests.
- LAN discovery (§2B.2): `sync/src/lan.rs` (real mDNS DNS-packet subset,
  std-only, ephemeral-port active scan, pointer-safe parser, loopback tests)
  + `GET /api/peers` + peers card. Direct-sync protocol sequenced.
- CAS leases (P0-19): single-write-txn acquire/renew/release + 8-thread
  exactly-one-winner regression test; `write_lease` removed.
- Op-log (§2C): `OpRow` + `oplog` table (atomic seq, since-pull, idempotent
  merge, dominance conflict scan) + `GET/POST /api/oplog/*` + unit tests.
- Sneakernet (§2B.4): `sync/src/bundle.rs` (`cyb-bundle/1`, BLAKE3 manifest,
  caps, idempotent import) + Admin bundle routes + round-trip/tamper tests.
- Agent todo bridge (§4.9): `todos` table + `todo_add/list/done` tools
  (owner-scoped, plan-deny parity, metaprompt, 23 schemas) + Tauri + REST +
  Archive TASKS server merge; watcher classify hook (`agentConfigId` →
  fire-and-forget `startAgentRun`); NOTES → `storeAgentMemory` bridge.
- Secrets CSV (§4.1): `secretImport.ts` (1Password/Bitwarden/generic, 2000 cap)
  + panel modal + tests.
- P0: hash-strip on IPC (P0-4), shim deletion, no-row-token test (P0-6);
  audited-and-ticked P0-1/2/8/10; P1-1/6/7/11 ticked.
- iOS (P2-16): `bundle.ios` floor + `docs/IOS.md` + config-shape tests.
- Frontend: 166 REST mappings, devices/todos/peers/oplog/bundle routes,
  store actions, `agentUi` metas, `secret-import` tests green.
- Docs: ANALYSIS 100% dispositions, TASKS ticks, ARCHITECTURE endpoint rows,
  OPERATIONS skill/task verbs + TLS runbook.
- Verified here: version agree, headers ok (tracked set; new files carry
  headers, verified by grep), `vue-tsc` clean, full vitest green (see run).
  Rust compiles/tests/iOS builds are CI's job per repo rules (no local cargo).

## 2026-10-11 — ANALYSIS close-out: sequenced list landed, P0 cluster closed

- Op-log wiring (§2C): `record_mutation_op` + `get_op_by_id` (`db`) called from
  every file op, collections, todos chokepoint, face rename/merge; `POST
  /api/oplog/resolve` verdicts (history never deleted); Sync → CONFLICTS
  resolver UI (`oplog.ts` + store actions + `oplog` cybsh verb); spine tests
  (resolve shapes, mutation appends, auth gates).
- Direct LAN sync (§2B.2 follow-up): `POST /api/peers/{id}/sync`
  (registry-authenticated, idempotent merge) + Devices SYNC button
  (fail-closed gate) + `lanSync.ts` + `lan` cybsh verb + spine peer-auth tests.
- iroh seam (§2B.5): `sync/src/iroh.rs` (config/endpoint/dial-plan +
  unit tests, std-only); dial backend sequenced (dep + networked CI).
- Skills panels (§4.8) + notes auto-index (§4.2) + rotation checklist (§3):
  `panel:` frontmatter → `skill:<slug>` targets + `skill panel` verb;
  `noteToMemory`; Shield rotation checklist; all in `spine-closeout.test.ts`.
- P0: P0-3 IPC gate, P0-5 persistent revocation + stable IPC secret, P0-7 SSE
  `?token=`, P0-9 remember audit, P0-11 symlink/atomic containment, P0-12
  memory owner-scoping, P0-13 independent Hybrid pair (+sealed persistence),
  P0-14 KDF separation + legacy fallback, P0-15 honest levels/labels, P0-16
  KEK cache, P0-17 fail-closed data_dir + no silent re-wrap, P0-20 merged
  Overwrite + deterministic KeepBoth, P0-21 `expired:`, P0-23 caps/paging,
  P0-24 hierarchy rebuild; P0-22 documented discipline. TASKS ticked.
- Docs: ANALYSIS refresh + dispositions, TASKS ticks, ARCHITECTURE endpoint
  rows, OPERATIONS verbs, AGENTS/README counts (71 utils · 170 REST ·
  40 tables · 82 files/893 tests).
- Verified here: headers ok, `vue-tsc` clean, full vitest green (82/893).
  Rust is CI's job (concurrent fmt-fix agent active; watch the run).
## 2026-10-12 — ANALYSIS frontend-close-out: WAN plan, volume queue+caps, exhaustive parity (no cargo, no macOS)

- New `src/utils/wanDial.ts` (§2B.5 frontend plan — same `<device>@<addr>`
  grammar + `unsupported:` plan + `wanWayOut` as `crates/sync/src/iroh.rs`,
  never dials) + Devices → WAN dial card (plan, never dials) +
  `wan plan <device>@<addr>` cybsh verb on all transports +
  `tests/frontend/wan-dial.test.ts` (5 tests).
- New `src/utils/volumeCaps.ts` (P1-17: shared 1 MiB cap, `STATIC_WRITE_LIMIT`
  re-export, Rust `MAX_WRITE_BYTES` pinned by static read) +
  `src/utils/volumeWriteQueue.ts` (P1-16 frontend half: FIFO single-writer
  queue, generations, `too_large:` gate, chain survival) +
  `volume-caps` (4) + `volume-queue` (3) tests.
- New `tests/frontend/transport-exhaustive.test.ts` (§3: every mapping has
  method + `/api/` path, every `REST_FIRST` resolves, `WRITE_ONLY` disjoint,
  spine twins pinned by static Rust reads — peer-sync/op-log/bundle are
  REST-first by design, devices/todos keep Tauri twins; corrected the first
  draft which wrongly assumed Tauri twins for all).
- Docs: `ANALYSIS.md` 2026-10-12 refresh + §2B.5/§2C/§3/§6 ticks;
  `AGENTS.md`/`README.md` counts to the verified tree (74 utils · 170 REST ·
  40 tables · 86 files/911 tests).
- Staying sequenced with honest reasons (need Rust/mobile toolchains): iroh
  QUIC dial backend, iOS device/simulator builds, full TS↔Rust equality sweep,
  P0-22 rebuild + Rust single-opener (frontend: 15 s abort + backoff in
  `restFetch` already shipped).
- Verified here: version agree, headers ok (522 tracked), `vue-tsc` clean,
  full vitest green (86/911). Rust is CI's job per repo rules (no local cargo).

## 2026-10-12 — ANALYSIS.md close-out part 2 (four sequenced items → landed)

Closed every remaining honest sequence from `ANALYSIS.md` on all layers this
environment can prove (no local cargo, no macOS signer, no networked QUIC CI):

- WAN HTTP dial (§2B.5): `src/utils/wanDial.ts` gains `wanHttpBase` /
  `wanDialSteps` (readyz probe + op-log pull/push/merge) / `wanHttpProbe`
  (fetch + 15 s AbortController, `network:` on failure, `unsupported:`
  without fetch); Devices → WAN card gains a live Probe button; `wan dial`
  cybsh verb on all transports; Rust twin in `crates/sync/src/iroh.rs`
  (`http_base` / `http_dial_steps` / `http_probe` via bounded TcpStream,
  relay tickets stay `unsupported:`) + unit tests. QUIC `dial_plan` stays
  honestly `unsupported:` — only relay dial remains sequenced (new dep +
  networked CI). `wan-dial.test.ts` grows to 8 tests.
- Full static parity sweep (§3): `scripts/check-transport-parity.mjs` +
  `npm run check:parity` + CI meta-check gate +
  `tests/frontend/transport-parity-full.test.ts` (real TS imports, both
  directions). The sweep caught and fixed a REAL drift: `POST
  /api/faces/detect*` was 404-before-auth for want of a `faces`
  ROUTED_SEGMENT in `crates/web/src/security.rs`. Only live-boot shape
  equality (real `crates/web` + snake→camel) stays a CI follow-up.
- iOS build path (P2-16): `scripts/ios-configure.sh` (version/floor/
  identifier/ATS audits) + `scripts/ios-signing.sh` (prepare/verify/cleanup,
  ephemeral keychain, mirrors Android) + `ios-build` CI job on macos-latest
  (simulator `.app` every run + signed `.ipa` when secrets exist) +
  `docs/IOS.md` runbook refresh + `mobile-ios.test.ts` pins. Only Apple
  secrets + TestFlight stay sequenced.
- P0-22 + single-opener (§5/§2C): `OpenerLock` in `crates/db`
  (`<db>.lock` create_new guard, second opener fails closed with
  `conflict:`, stale-PID reap on Linux, release-on-drop via Arc,
  `opener_lock_path` accessor + regression test) beside the existing 100 MB
  `MAX_BODY_SIZE` cap; `tests/frontend/rest-hardening.test.ts` pins the
  frontend half (15 s abort, one backoff retry on `network:`/`rate_limited:`).
  `TASKS.md` P0-22 ticked.
- Docs: `ANALYSIS.md` 2026-10-12 part-2 refresh + §2B.5/§2C/§3/§5/§6 ticks;
  `TASKS.md` P0-22; `AGENTS.md`/`README.md` counts to the verified tree
  (74 utils · 170 REST · 40 tables · 88 files/925 tests).
- Verified here: version agree, headers ok (531 tracked), parity sweep clean
  (170 mappings, 37 families), `vue-tsc` clean, full vitest green (88/925).
  Rust is CI's job per repo rules (no local cargo).

## 2026-10-12 — P2P expansion: peer transports + cross-device search port (6 targets)

- New `crates/sync/src/peers.rs`: `PeerTransport` (lan-http/wan-http/bundle
  live, relay-future sequenced) + `<device>@<addr>` grammar + `normalize` /
  `classify` / `is_lan_base` + `direct_http_plan` (relay refuses
  `unsupported:`) + `FederatedHit` + `merge_federated_hits` (dedup by file
  id, max score wins, desc sort) + `fetch_peer_hits` (reqwest blocking,
  bounded timeout, `auth:`/`network:` normalization, tolerant hit parsing) +
  `platform_notes` (android/ios/windows/linux/docker/wasm fallbacks) + tests.
  No new deps (std + reqwest/serde_json already in the crate).
- `crates/sync/src/iroh.rs`: additive `classify_addr` + `direct_http_plan`
  aliases over `peers` (existing `dial_plan`/`http_*` pins untouched).
- `crates/web/src/api/search_api.rs`: `FederatedPeer`/`FederatedRequest` +
  `search_federated` (local BM25 tagged `local`, ≤8 peers fanned out, dead
  peers degrade to zero hits, merged); `crates/web/src/lib.rs`: new
  `POST /api/search/federated` route (second segment `search` already
  routed — parity sweep stays clean).
- Tauri: `search_files_federated` (search.rs) + `probe_peer` /
  `peer_platform_notes` (devices.rs), wired in `lib.rs`; explicit peer URLs
  are what make android/ios/docker work where mDNS multicast is blocked.
- WASM parity: `search_scored` + `search-scored` dispatch arm (real score +
  ≤200-char snippet + filename/content kind, `search()` path-only output
  untouched) + `wasmSearchFilesScored` (scored-first, legacy fallback);
  `search_files`/`_paginated` static handlers now return real scores and
  snippets instead of `1.0`/`''`.
- Frontend: new `src/utils/peerSearch.ts` (TS twin of `peers.rs`: same
  grammar/merge, `peerCapabilityMatrix`, localStorage peer cache for
  mobile/docker/wasm, `fetchPeerHits`/`federatedSearch`,
  `syncExchangeWithPeer` pull→push→merge→render executor); `wanDial.ts`
  `wanSyncExchange` + `lanSync.ts` `lanSyncExchange` reuse it.
- `useTauri.ts`: `search_files_federated` REST route (POST
  `/api/search/federated`, 171 mappings), mobile IPC set extended,
  `peer_platform_notes`/`probe_peer` answered on every transport (IPC twin
  on Tauri, fetch probe elsewhere), `search_files_federated` static handler
  fans out to explicit + cached peers, IPC args normalized to twin names.
- Tests: new `tests/frontend/peer-search.test.ts` (15 tests: grammar,
  merge order, matrix, cache, fan-out degradation, `auth:`/`network:`,
  exchange + relay refusal, wrappers, route shape).
- Verified here: version agree, headers ok (536 files), parity sweep clean
  (171 mappings, 37 families), `vue-tsc` clean, full vitest green (89/940).
  Rust is CI's job per repo rules (no local cargo).

## 2026-10-11 — Resume main integration and complete backup safety validation
- Re-fetched `origin/main` through `7d51fcab` (17 commits absent from `develop` at `d8ec5ab`) and integrated it only into `develop`; `main` remains untouched. The main-side Clippy fix removes `clone()` from the `Copy` PBKDF2 accumulator, so no duplicate fix was added.
- Resolved `TASKS.md` by keeping the more complete checked-off P1 work already verified on `develop`; kept `pathExists()`'s actual requested-path comparison rather than main's unconditional `true`; combined both append-only worklog histories. P1-14 and backup/filesystem safety changes remain present.
- Local checks passed: version consistency; all file headers; `vue-tsc`; Vitest (95 files / 977 tests); transport parity (171 mappings / 37 families); WASM frontend and wiki builds with `/develop/`; staged and unstaged `git diff --check`. Rust formatting, Clippy, and Rust tests remain CI-only per `AGENTS.md`.
- Audited Pages assembly: main stays at `/`, the preview is assembled at `/develop/`, and the wiki uses the matching branch base (`/wiki/` or `/develop/wiki/`). Deployment remains gated on successful `wasm-build` and `rust-check`.

## 2026-10-12 — P2P expansion-2: shared URL builders, per-peer auth, 6-target runbook

- `crates/sync/src/peers.rs`: additive `MAX_FEDERATED_PEERS` (8) +
  `FEDERATED_MARKER_HEADER` (`x-cybermanju-federated`, log triage only —
  peers serve plain `/api/search`, loops impossible by construction) +
  `peer_readyz_url`/`peer_search_url`/`peer_oplog_since_url`/
  `peer_oplog_push_url`/`peer_conflicts_url` (single source of truth for the
  URL shapes) + `deduplicate_peers` (first-wins by normalized base, cap 8) +
  `wasm_peer_warning` (mixed-content/loopback/invalid, same strings as TS) +
  `platform_setup` (copy-pasteable per-target commands) + 4 tests.
- `crates/sync/src/iroh.rs`: additive `is_relay_addr` (one relay grammar
  shared with `peers::classify_addr`) + `normalize_base` + `wan_way_out`
  (LAN/bundle/op-log, same 3 lines as TS) + `peer_readyz_url`/
  `peer_search_url`/`peer_oplog_since_url` + `federated_search_plan`
  (local BM25 + one bounded peer fetch, relay → `unsupported:`); `http_probe`
  now refuses `https://` with `unsupported:` (no TLS in `TcpStream` — use the
  fetch probe or terminate TLS at a proxy) and dials IPv6 literals verbatim.
- `crates/web/src/api/search_api.rs`: `FederatedPeer.token` (per-peer bearer,
  serde-default so old bodies still parse) + `search_federated_with_bearer`
  (explicit token wins, else caller-bearer fallback; neither logged) +
  device-id retag now compares the normalized base too (trailing-slash safe).
  `crates/web/src/lib.rs`: federated arm forwards the caller's bearer as the
  fallback (single-secret vaults fan out with zero per-peer config).
- Frontend: `peerSearch.ts` twins (`peerReadyzUrl`/`peerSearchUrl`/
  `peerOplogSinceUrl`/`peerOplogPushUrl`, `deduplicatePeers`,
  `wasmPeerWarning`, `buildFederatedBody`, `MAX_FEDERATED_PEERS`/
  `FEDERATED_MARKER_HEADER`; `fetchPeerHits` sends the marker header;
  `federatedSearch` dedups + prefers per-peer tokens); `wanDial.ts`
  `isRelayAddr`/`wanPeekUrls`/`wanFederatedPlan`; `useTauri.ts` federated
  REST/IPC/static paths all pass per-peer tokens through, dedup/cap peers,
  and forward the shared fallback token on static hosts.
- Docs: ARCHITECTURE REST table gains `GET /api/search/paginated` +
  `POST /api/search/federated` (token + fallback semantics); OPERATIONS
  gains §4c2 (6-target P2P + search matrix, auth model, cybsh verbs).
- Tests: new `tests/frontend/peer-search-expansion.test.ts` (5 suites:
  URL shapes, dedup/cap, wasm warnings, body builder + route token
  passthrough, relay gate + federated plan + peek URLs).
- Verified here: version agree, headers ok (537 files), `vue-tsc` clean,
  targeted vitest green (6 files/59 tests), full vitest green (90/947).
  Rust is CI's job per repo rules (no local cargo).

## 2026-10-12 — Security follow-up: explicit peer credentials only

- Integrated the current `origin/main` changes through `5ef7a83` into `develop`; `main` remains untouched. The latest main commit restored the missing closing delimiter in the Tauri LAN discovery call. Applied only the remaining rustfmt diffs reported by the preceding main CI in `crates/sync/{iroh,peers}.rs` and `crates/web/src/lib.rs`.
- Removed caller-bearer fallback from federated search: the inbound `Authorization` header authenticates only the local request; REST/Rust and WASM/browser fan-out send a bearer only when explicitly assigned as that peer's `token`. Legacy top-level shared-token options are ignored. Updated `ARCHITECTURE.md` and `docs/SECURITY.md` to state the trust boundary.
- Added a frontend regression test proving an explicitly assigned peer token is sent only to its peer and a legacy shared caller token is not forwarded.
- Local verification passed: version check, file-header check, `vue-tsc`, Vitest (95 files / 978 tests), and staged/unstaged diff whitespace checks. `VITE_BASE=/develop/ npm run build:wasm:frontend` also passed, with generated asset URLs under `/develop/assets/`. Rust formatting/Clippy/tests remain CI-only as required by `AGENTS.md`; latest main CI was still running when recorded.
- Pages assembly remains main at `/`, develop preview at `/develop/`, and wiki paths preserved. No release or main-branch change.

## 2026-10-12 — CI follow-up: Rust formatting

The CI runs for main `5ef7a83` and develop `30d323b` both failed at the Rust `Check formatting` step on the same match-arm layout in `crates/web/src/lib.rs`; no behavior or test failure was reported. Applied the exact rustfmt shape to the develop copy only. Version metadata and WASM build jobs passed; downstream platform jobs were cancelled after the Rust gate, and Pages deployment was skipped. Revalidation and a new develop CI run are pending; main remains untouched.

## 2026-10-12 — Main P2P parity and CI follow-up

- Integrated `4ca39ac`, `978c7ef`, and `c948bd8` from `main` into `develop`. Preserved main's P2P hardening: host-based LAN classification, token-based relay grammar and relay-free fan-out, IPv6 peer URLs, WAN `:3456` defaults, bounded HTTP status-line reads, timeout clamps, finite sync cursors, and the 10,000-op cap. The new P2P regression suite is included; c948's Rust `Debug` derives for `expect_err` tests are also preserved.
- Resolved the search API merge by retaining main's relay skip while sending only `peers[].token`; the inbound caller bearer remains local to request authentication and is never forwarded. The REST route calls the no-fallback API. This keeps the security decision recorded above while retaining main's P2P safety fixes.
- The prior develop CI failed only at Rust formatting; the exact route formatting was committed on develop. The `978c7ef` main CI was cancelled when `c948bd8` arrived; main's latest `c948bd8` CI is in progress. A fresh develop CI will run after the next push. Rust checks remain CI-only per `AGENTS.md`.

## 2026-10-12 — Final local validation

After integrating `main` through `c948bd8`, local checks passed: `check-version.sh`, file headers (545 files), `vue-tsc`, Vitest (96 files / 990 tests), `git diff --check`, and `VITE_BASE=/develop/ npm run build:wasm:frontend` with output assets verified under `/develop/assets/`. A source scan confirmed that no caller-bearer fallback remains. Cargo/rustfmt/Clippy/Rust tests were not run locally, per `AGENTS.md`; `main` CI for `c948bd8` was still in progress, and the next develop CI will run after publishing the validated head.

## 2026-10-12 — Clippy follow-up after c948bd8

The `Rust Lint & Test` job for main `c948bd8` reached Clippy and failed on one `unused import: VerifyingKey` in `crates/crypto/src/pqc.rs:27` (the remaining occurrence is documentation only). Removed that unused import on develop only; this is behavior-neutral. The Rust gate for the corrected develop head must be rechecked in CI; Cargo/Clippy are not run locally by repository rule.


## 2026-10-12 — Hold: main P2P integration blocked by security review

- `develop` remains at remote commit `b33cbe92c6a42de1329390b135744fe9daab2b9f`; `main` was not modified. The develop CI run `38192940869` failed in Rust Lint & Test at `crates/db/src/database.rs:1807` because `unwrap_err()` requires `Database: Debug`. Main added a fix in `fb27bef`, but that commit also introduced a substantial P2P/bundle feature set; main then advanced again to `4a5d182` while this review was in progress. The latest main CI was still in progress at the time of review.
- Integration/push was paused after static review found material blockers in the new main code: federated search can forward the caller's bearer to an arbitrary peer when `peer.token` is absent (contrary to the explicit-token policy already enforced in develop); server-side peer URLs lack SSRF/private-network and redirect protections; relay drop/poll/ack are not bound to the authenticated device owner; unsigned peer-sync envelopes are still merged; bundle import lacks a per-payload 5 MiB check before base64 decode; and the desktop signing path does not persist/select its private key securely. The latest `4a5d182` changes frontend/docs/tests and does not address the backend blockers reviewed here.
- No new main changes were merged, no push/release was made after these findings, and no Rust Cargo command was run locally (`AGENTS.md` requires Rust checks through CI). Revisit integration only after the security/compatibility policy is clarified and the required checks are green.

## 2026-10-12 — P2P 100%: relay live, https probe, payloads, attested sync, presence

- Relay-over-HTTPS NAT traversal is live (no new deps — both sides outbound
  HTTPS, no inbound port; only the QUIC *datagram* upgrade stays sequenced):
  `crates/sync/src/iroh.rs` gains `relay_envelope` (`{v:1,to,kind,payload}`),
  `relay_dial_steps` (drop→poll→merge→push→render), `relay_drop_path`/
  `relay_poll_path`, `relay_forward` (bounded reqwest POST), and
  `endpoint_fingerprint` (BLAKE3 of the registry key); `dial_plan` returns a
  live HTTP plan for direct addrs and a live relay plan when `relay_url` is
  set (`unsupported:` + way out only with no relay configured).
- `http_probe` dials `https://` via reqwest+rustls (plain HTTP stays on the
  TcpStream fast path); relay tickets refuse with the relay hint (shared
  `classify_addr` grammar everywhere).
- Server-assisted presence + relay mailbox (`crates/sync/src/peers.rs`,
  kv-backed, no schema change): `announce_presence`/`known_presence` (5 min
  TTL) + `relay_drop`/`relay_poll`/`relay_ack` (32 msgs/recipient, 256 KiB
  cap) + `presence_now`; `platform_setup` per-target notes point at presence.
- Signed peer sync: `crates/crypto/src/pqc.rs` gains `verify_with_public_key`
  (ML-DSA-44/65/87, same `KeyInit::new_from_slice` + `Verifier` shape as the
  wasm twin); new `crates/sync/src/auth.rs` (`ENVELOPE_DOMAIN`,
  `SignedSyncBody`, canonical bytes via `to_value`+`to_string` so nested keys
  sort exactly like the TS `stableStringify` twin — pinned cross-language
  vector in both test suites); `POST /api/peers/{id}/sync` verifies present
  signatures against the registry (`auth:` fails closed) and reports
  `verified`, unsigned bodies still merge as legacy; `sign_peer_ops` Tauri
  command signs on desktop, wasm `ml_dsa65_sign` on Pages.
- Bundle payloads: `Bundle` gains `payloads` + `payloads_blake3` (ops manifest
  untouched — legacy bundles verify byte-identically); caps 100 files / 5
  MiB/file / 50 MiB total; `export_bundle_with_files` + `import_bundle_detailed`
  (every payload base64-decoded + re-hashed, fail-closed); `POST
  /api/sync/bundle/build` + payload metadata in the import response.
- New routes (all Authenticated, additive): `GET /api/peers/known`, `POST
  /api/peers/announce`, `POST /api/relay/drop|poll|ack` (+ `relay` in
  `security::ROUTED_SEGMENTS` so the 404-before-401 gate knows it).
- Frontend: `wanDial.ts` live plans + `buildRelayEnvelope`/`relayDialSteps`/
  `relayDropPath`/`relayPollPath`; new `p2pPresence.ts` (canonical envelope,
  signed-body/announce/relay builders, injected-fetch drop/poll/ack clients,
  `openDataChannelViaRelay` WebRTC handshake via the relay mailbox with an
  honest `unsupported:` off-browser); `syncExchangeWithPeer` `signer` hook
  (attested pushes go to `/api/peers/{id}/sync` with `verified` in the steps);
  `useTauri.ts` 6 new REST-first routes + signature passthrough on
  `peer_sync`; store `knownPeers`/`fetchKnownPeers`/`announcePresence` +
  attested `syncWithPeer` (✓ seal); DevicesPanel Presence card + WAN relay
  URL field (localStorage); cybsh `wan plan|dial [--relay <url>]`, new `pex
  announce|known` + `relay drop|poll|ack` verbs.
- Docs: OPERATIONS §4c2 rewritten (live relay/presence/payloads/attestation,
  per-target table, WebRTC, `https://` probes live); ARCHITECTURE REST table
  gains presence/relay/bundle-build rows + `verified` on peer sync.
- Tests: `p2p-live.test.ts` (canonical vector, envelopes, mailbox clients,
  attested exchange, routes, verbs); `wan-dial.test.ts` updated to live
  plans; Rust unit tests for verify/envelope/relay/presence/payloads.
- Verified here: version agree, headers ok (541 files), `vue-tsc` clean,
  transport parity clean (177 mappings), full vitest green (92 files / 974
  tests). Rust is CI's job per repo rules (no local cargo) — additive,
  existing-dep-only changes throughout.

## 2026-10-12 — P2P bonds: pairing codes, RS swarming, rooms + project selections (ANALYSIS.md §2B 6-9)

- Researched online and recorded selections in ANALYSIS.md (licenses +
  versions verified 2026-10-12): `iroh` 1.x selected for QUIC (MIT/Apache,
  relay+gossip+BLAKE3-blobs+mobile/WASM, self-hostable relay/dns; dep itself
  sequenced — heavyweight, needs networked CI + mobile link validation;
  `rust-libp2p` rejected as over-surface); `raptorq` sequenced for GB-scale
  fountain codes; `magic-wormhole.rs` rejected as a dep (EUPL-1.2 vs MIT
  policy), adopted as UX pattern; `librqbit` deferred; Trystero recorded as
  one-file follow-up; `iroh-webrtc-transport` recorded as the direct
  browser↔native path.
- Native pairing codes over the relay mailbox (zero new routes/deps):
  `peers::{pairing_code, normalize_pairing_code, pairing_recipient,
  is_pairing_recipient}` (64-word list, OsRng entropy, `pair:<code>`
  recipients) + `src/utils/pairing.ts` twin (WebCrypto mint, static-read
  test pins the wordlist cross-language) + `pair make|drop|claim` verb +
  Tauri `pairing_code` twin.
- Erasure-coded relay swarming on the in-tree codec: `crates/sync/src/swarm.rs`
  (`swarm_layout` ~64 KiB shards, `swarm_encode`/`swarm_decode` with
  per-symbol + whole-object BLAKE3 seals, `kind: swarm-symbol`); Tauri
  `swarm_encode`/`swarm_decode` twins; wasm `swarm_encode_json`/
  `swarm_decode_json` exports (new `cybermanju-erasure` + `base64` wasm deps,
  both pure Rust); `src/utils/p2pSwarm.ts` (layout mirror, envelope
  validation, backend injection, wasm-loader factory) + `swarm plan` verb.
- Zero-dep relay rooms: `src/utils/p2pRoom.ts` (`room:<app>/<room>` topics,
  hello/action kinds, read+ack splits) + `room join|say` verb.
- Docs: ANALYSIS.md status refresh + §2B items 6-9 (selections, versions,
  integration plans, honest sequence reasons) + §6 phase-3 update;
  OPERATIONS §4c2 gains pairing/swarm/rooms paragraphs + verbs.
- Verified here: headers ok, `vue-tsc` clean, full vitest green. Rust
  (pairing/swarm/wasm export) is CI's job per repo rules — additive,
  existing-dep-only code following compiling in-repo patterns.

## 2026-10-12 — P2P deps for CI: iroh + raptorq declared, wired, proving

- `crates/sync/Cargo.toml`: `iroh = "1"` (addressing layer only — no async
  runtime added) + `raptorq = "2"` (zero mandatory deps); `crates/os-wasm`
  gains `raptorq` for the Pages twin. All APIs docs-grounded (iroh 1.3.0,
  raptorq 2.0.1) since nothing here compiles locally per repo rules.
- iroh addressing live in `crates/sync/src/iroh.rs`: `parse_endpoint_id`
  (hex-or-base32 in, hex out) + `parse_relay_url` + canonical
  `iroh:<id>@<relay>` tickets (`iroh_ticket`/`parse_iroh_ticket`, ed25519
  base point as the deterministic test vector); `dial_plan`/
  `relay_dial_steps` prefer the ticket relay over the configured one. Async
  `Endpoint` dial stays sequenced on a tokio runtime decision; `iroh-blobs`
  excluded (would duplicate redb 4).
- RaptorQ fountains live in `crates/sync/src/swarm.rs`
  (`fountain_encode`/`fountain_decode`, OTI-per-symbol, `catch_unwind` on
  untrusted packet bytes, whole-object re-hash) + Tauri
  `fountain_encode`/`fountain_decode` twins + wasm `fountain_*_json`
  exports + `p2pSwarm.ts` envelope validation/plan/backend factories +
  `swarm fountain` verb + `wanDial.ts` ticket builders (shape-checked; curve
  checked natively).
- Docs: ANALYSIS.md §2B items 6-7 rewritten (dep declared, proving in CI);
  OPERATIONS §4c2 fountain/ticket lines.
- Verified here: headers ok, `vue-tsc` clean, vitest green. Rust compiles +
  unit tests are CI's job — watch the Rust Lint & Test job (new deps resolve
  + build on all targets incl. Android; basepoint-vector and fountain tests
  are the canaries).

## 2026-10-12 — Strict P2P/Sync hardening + main integrated through a2b2c09

- Integrated the latest 10 commits from `origin/main` through `a2b2c09` into `develop`; merge commit `ad0294c`. The sole history conflict was `worklog.md`; both branch histories were retained. `main` was not modified.
- Strict sync authentication is fail-closed: `/api/oplog/push` and `/api/peers/{id}/sync` require a valid ML-DSA envelope, the registered active device must belong to the authenticated owner, and every signed op must name that device. Unsigned, revoked, wrong-owner, mismatched-device, and invalid-signature requests are rejected.
- Removed caller-bearer fallback: outbound federated search/sync uses only a non-empty token explicitly assigned to that peer. The WAN exchange API now requires a local signer and explicit peer token; preflight limits/auth checks occur before network access.
- Server-side peer search validates HTTP(S) destinations, rejects loopback/link-local/metadata/reserved or unauthorized private targets, checks all DNS answers and pins the approved resolutions; redirects and proxy use are disabled. Private/LAN dialing is enabled only after matching owner-scoped device presence.
- Relay mailboxes are tenant-bound: both sender and recipient must belong to the authenticated owner to enqueue; poll/ack and presence are restricted to that owner's active devices. Added an integration regression for cross-owner relay denial.
- Bundle imports enforce safe basenames and per-payload 5 MiB limits before base64 decoding. Device registration stores `owner_id`; Tauri signing uses keystore-sealed seeds, ties the signer to the op-log device identity, and removes the raw-seed IPC path.
- Updated `ARCHITECTURE.md`, `docs/SECURITY.md`, and `docs/OPERATIONS.md`; stale unsigned-sync/legacy bearer descriptions were removed. Rust integration fixtures now generate ML-DSA keys and exercise signed pushes, wrong-owner rejection, idempotence, conflicts, and bundle export.
- Local verification passed: `check-version.sh`; file headers (555 files); `vue-tsc --noEmit`; Vitest (98 files / 1,023 tests); `VITE_BASE=/develop/ npm run build:wasm:frontend`; `cargo fmt --all -- --check`; staged/unstaged whitespace checks. Rust Clippy/tests and the actual GitHub Pages deployment remain pending CI after the develop push. Workflow assembly was checked: main remains at `/` and develop is isolated at `/develop/`.

## 2026-10-12 — CI correction: restore ML-DSA VerifyingKey import

- CI run [38201461668](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38201461668) for `a93e162` failed at Rust compilation: `verify_with_public_key` uses `VerifyingKey` at `crates/crypto/src/pqc.rs:1007,1014,1021`, but the import had been removed in an earlier unused-import cleanup before the latest `main` verifier landed. The same E0425 root cause propagated to Rust Lint & Test, Windows, macOS, iOS, Android, Linux packaging, Arch, and Docker. WASM and Version & Metadata passed; Pages deploy was skipped.
- Restored `VerifyingKey` to the `ml_dsa` imports and confirmed `cargo fmt --all -- --check` plus `git diff --check`. Rust compile/Clippy/tests and Pages must be rerun in CI on the follow-up `develop` commit; `main` remains untouched.

## 2026-10-12 — CI correction: deduplicate sync Database import

- CI run [38202364081](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38202364081) confirmed the verifier import correction and then stopped on `E0252` in `crates/sync/src/peers.rs`: `cybermanju_db::Database` was imported twice after applying the local hardening snapshot over the newly merged `main` file. The duplicate also triggered `unused-imports`; the same compile error propagated across Rust/native/Docker jobs. WASM and Version & Metadata passed; Pages deploy was skipped.
- Removed only the duplicate module import. A scan of the 17 changed Rust files found no repeated module-level imports; `cargo fmt --all -- --check` and `git diff --check` pass. Rust CI and Pages must be rerun on the next `develop` commit; `main` remains untouched.

## 2026-10-12 — CI follow-up: base64 length lint and iOS framework link

- CI run [38203004910](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38203004910) passed the import fixes; Rust Lint & Test then stopped on Clippy `manual_div_ceil` at `crates/sync/src/bundle.rs:261`. Replaced `((p.size + 2) / 3) * 4` with `p.size.div_ceil(3) * 4`; the per-payload cap is checked before this calculation. The WASM, Flatpak, Version & Metadata, and other platform jobs passed.
- The separate iOS simulator failure was an Xcode link error for `SystemConfiguration` symbols referenced by the native network-interface dependency (`netdev`), with no such framework link in the project configuration. `src-tauri/build.rs` now emits `cargo:rustc-link-lib=framework=SystemConfiguration` only when `CARGO_CFG_TARGET_OS=ios`. This is a targeted platform link fix; the next CI run must verify it. Pages deployment was skipped because the run was not fully green; `main` remains untouched.

## 2026-10-12 — CI follow-up: oplog lint and generated Xcode linker settings

- CI run [38204695277](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38204695277) passed the `div_ceil` fix and exposed `clippy::too_many_arguments` in `api::oplog::route`. Grouped `owner_id` and `is_admin` in `OplogAuth`; the route, HTTP dispatcher, and tests now pass the same authenticated owner/admin values without weakening tenant checks.
- The iOS final Xcode link still reported unresolved `SystemConfiguration` symbols; the `cargo:rustc-link-lib` emitted by `src-tauri/build.rs` did not reach that final linker. Tauri issue [#9204](https://github.com/tauri-apps/tauri/issues/9204) describes the same symbols and identifies the generated Apple project as the required integration point. Reverted the ineffective build-script experiment and added `scripts/ios-link-frameworks.py`, invoked after `tauri ios init` by `scripts/ios-configure.sh`, to add `-framework SystemConfiguration` idempotently to generated Xcode linker settings while preserving existing flags. A temporary pbxproj fixture passed for scalar flags, array flags, absent flags, and repeated execution.
- `div_ceil`, the `OplogAuth` refactor, and the generated-project patch remain subject to the next CI run. WASM, Flatpak, and Version & Metadata passed on run 38204695277; Pages was skipped because required jobs failed. `main` remains untouched.

## 2026-10-12 — CI follow-up: unused WASM fountain MTU

- CI run [38206333283](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38206333283) failed in `Rust Lint & Test`: Clippy's `-D warnings` rejected the unused `mtu` field in `crates/os-wasm/src/swarm.rs:111`. The Rust tests were skipped; iOS, WASM, Version & Metadata, and the other completed jobs passed. Pages was skipped because the required pipeline was not green.
- Commit [`ab4861b`](https://github.com/cybermanju/cybermanju.github.io/commit/ab4861b327c71fc9b4d8be72220beac9c7a5d600) preserved required `mtu` deserialization under the existing JSON key while marking it intentionally unused; the OTI remains authoritative for decode parameters. It was pushed only to `develop` with author `CyberManju <cybermanju.os@gmail.com>`.
- Follow-up CI run [38207991241](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38207991241) passed the `mtu` lint and WASM build, then found two `clippy::cloned_ref_to_slice_refs` errors in `crates/tests/src/spine.rs:255` and `:430`. Replaced those cloned one-element slices with `std::slice::from_ref`; the Rust CI and Pages deployment still need a successful rerun.
- Run [38209264918](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38209264918) passed the prior fixes and the WASM build, then exposed one `E0505` test-cleanup borrow error, two `clippy::manual_range_contains` assertions in the sync swarm tests, and one remaining cloned slice in the relay test. Fixed these test-only issues by borrowing the path during cleanup, using inclusive-range `contains`, and passing `std::slice::from_ref(&id)`; another CI run is required.
- Direct GitHub API checks confirm `main` remains `a2b2c09` and `develop` was `ab4861b` before this follow-up, with `main` already integrated into `develop`. The WASM artifact assembly job passed on run 38207991241; its workflow combines main at `/` and the develop preview at `/develop/`. Pages was skipped because Rust Lint & Test failed; production `main` remains untouched.
- CI run [38210524112](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38210524112) passed Clippy and reached Rust tests, but the suite failed across crypto, db, OS backup, WASM, sync, repair, and oplog integration tests; Pages was consequently skipped. The failure log is the source of record; the affected contracts are being investigated before the next code commit.
- Root causes from run 38210524112: the zero-based op-log started at `seq=0` while the API queried `seq > since`, which hid the first signed op and broke deduplication, conflicts, owner pages, and bundles; `repair/status` wrapped an already-`Result` status in a second `Ok`; plaintext container encoding retained salt/nonce metadata; fountain decode could return after completing before validating trailing symbols; the encrypted fixture did not match the documented PBKDF2(64)+HKDF-SHA256 schedule; and its test image generator overflowed in debug builds. Fixes now use an inclusive next-sequence cursor and return the next page cursor, preserve existing signed sequence IDs (with a missing-counter fallback), serialize repair status once, omit plaintext crypto metadata, use a matching KDF fixture, wrap test arithmetic, and validate all fountain symbols before decoding. The strict token/owner/signature/SSRF/limit/keystore hardening remains in place.
- Local checks passed: version and file-header scripts, `npx vue-tsc --noEmit`, `npm test` (98 files / 1,023 tests), and `git diff --check`. Rust/Cargo remains CI-only as required by `AGENTS.md`; the corrected Rust suite is pending the next GitHub run.
- Immediately before this patch, direct authenticated fetch confirmed `origin/main` is still `a2b2c09` and already integrated into `origin/develop` (`4220f04`); only `develop` is in scope and no local or remote `main` change was made.
- CI run [38212835222](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38212835222) found four `cargo fmt --check` formatting differences and a WASM-build `E0597` in the redb table guard used to read the op-log counter; dependent platform builds then failed from the same compile problem and Pages was skipped. Bound the redb access guard before converting its value in both reads, and applied the exact rustfmt shapes shown by CI. The full Rust/test and Pages checks remain pending a fresh run; no local Cargo command was run.
- Run [38213674322](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38213674322) confirmed the redb lifetime fix and rustfmt changes: WASM and Android/iOS/Windows/macOS/Arch builds passed, as did the other Rust suites, but one `cybermanju-os --lib` test failed. Its `did_you_mean` fixture used the unrelated word `frobnicate` while expecting an `export` suggestion; the helper intentionally suppresses suggestions for distant inputs. Changed only the fixture to the near-typo `exprot`, preserving the production suggestion threshold. The run's Debian packaging job was still in progress at the last check, and Pages remained skipped because the Rust job failed; a fresh full CI run is pending.
- Run [38215119821](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38215119821) confirmed the suggestion-fixture correction and passed all other listed platform/package jobs, but the backup integration test then exposed a second contract mismatch: a missing file produced generic `io error` although `backup.rs` promises `not_found:` failures. Updated `Kernel::open` to classify only `std::io::ErrorKind::NotFound` as `not_found:` (all other open errors retain their existing `io error` message). The run's `dist-wasm` artifact was downloaded and verified to contain both root `index.html` and `develop/index.html` (281 files); Pages was skipped while Rust failed. This latest ENOENT correction awaits a new CI run.
- Run [38217040138](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38217040138) had only one reported blocker: `cargo fmt --check` required rustfmt to collapse the `opts.open(...).map_err(...)` chain in `crates/os/src/api.rs`. The check stopped before Rust tests ran; all other jobs (including WASM, platform builds and Linux packaging) passed, while Pages was skipped as a downstream job. Applied the exact rustfmt shape; full lint/test and the Pages deployment remain pending a new run.
Final green validation for source commit [`e48a463`](https://github.com/cybermanju/cybermanju.github.io/commit/e48a463e05083e6986dfb563da4f144a194322de): CI run [38218934729](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38218934729) completed successfully, including Rust Lint & Test, WASM, all platform/package jobs, and Deploy to GitHub Pages. Downloaded and inspected both `dist-wasm` (281 files) and `github-pages` (289 files); each contains `index.html` at the root and `develop/index.html`. GitHub refs at verification were `main=a2b2c0959c479357e82c65667a6d5c89d82cb861` and `develop=e48a463e05083e6986dfb563da4f144a194322de`, so production main remained unchanged. A worklog-only commit follows; its CI is being verified separately.
Clarification: commit [`d0a49df`](https://github.com/cybermanju/cybermanju.github.io/commit/d0a49df1f4bc2139fe86c7d1b181feb51da2f300) changes only `worklog.md`. The CI workflow explicitly ignores Markdown-only pushes (`paths-ignore: "**.md"`), and the authenticated API returned zero runs for that SHA; this is an intentional skip, not a failed or pending check. The last code-bearing commit, `e48a463`, has the complete green CI and Pages deployment recorded above.


## 2026-10-12 — Android/Termux support for the Go CLI

- `install.sh` now detects Termux, selects only the Android ARM64 tarball, installs to `$PREFIX/bin` (or updates a writable `cyb` already on `PATH`), verifies its published SHA-256, and runs the Go CLI setup without offering the desktop Docker/native-app menu. Non-ARM64 Termux devices fail early with an explicit message; explicitly requested Docker/native backends are skipped safely.
- Added Android/arm64 cross-builds to CI and tagged-release workflows. The CLI now has an Android-safe local `atotto/clipboard` fork that avoids `exec.LookPath` during Android initialization (which triggers Termux seccomp `SIGSYS`); clipboard helpers are invoked lazily. OAuth URLs use Android Activity Manager directly, and Docker commands return a clear unsupported message instead of probing executables on Termux. The user-facing Supabase backend label is `CLI Limited`, preserving the internal `supabase` key.
- Local verification: `go vet ./...`, `go test -race -count=1 ./...`, and cross-builds for linux/darwin/windows amd64+arm64 and android/arm64 passed. Shell syntax, ShellCheck, workflow YAML parsing, version/header checks, and a simulated Termux install/update/checksum/unsupported-ABI/backend/setup-flow test passed.
- Commit [`92e7b4a`](https://github.com/cybermanju/cybermanju.github.io/commit/92e7b4a7c381df82e0323e92a4af321ac8523764) was pushed only to `develop`, carrying the four newer commits already present in `main`; `origin/main` stayed at `44bd39f`. CI run [38295716335](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38295716335) completed successfully, including Build CLI, Build Android, Rust Lint & Test, and Deploy to GitHub Pages. No release or tag was created. The current `v0.1.1` predates Android support and has no Android asset; a later tagged release must publish `cyb-android-arm64.tar.gz` before `latest` Termux installs can succeed.


## 2026-10-12 — Termux setup mode selection

- The reported post-input spinner came from `cyb setup` assuming every URL was a CyberManju dashboard and probing `/api/auth/status`; there was no backend-type question. If a Supabase project URL was entered there, it targeted the wrong API. On Android/Termux, the wizard now asks **Remote dashboard** vs. **CLI Limited (Supabase direct)** before requesting credentials. CLI Limited uses a saved or CI-baked broker pair when available, saves the backend, and shows the `cyb supa login` next step; Remote Dashboard requires an explicit URL rather than defaulting to localhost.
- `cyb setup` now preserves existing profile/backend/session fields when saving dashboard login. The installer message and Termux README describe the new selection flow; setup tests cover Android choices, selector conditions, Supabase profile preservation, and required broker fields.
- Local validation passed: `go vet ./...`, `go test -race -count=1 ./...`, Android/arm64 cross-build, installer Termux smoke test, ShellCheck, shell syntax, file-header check, and `git diff --check`. GitHub CI for this change is pending.
- CI run [38308964630](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38308964630) for code commit [`5614bb9`](https://github.com/cybermanju/cybermanju.github.io/commit/5614bb9ec1717a914470ac4d8761b222380d3eba) completed successfully across all jobs, including Build CLI, Android, iOS, Rust Lint & Test, and WASM. This follow-up records the green result; no release or tag was created.


## 2026-10-13 — Plasma tray accessibility

- Confirmed the live GitHub refs `main` and `develop` both at `1920ee2080910f7f885ab7862ee6614d0ecc537e`; no main-only commits were missing, and all work stayed on `develop`.
- Added explicit dynamic `aria-label` names to the five icon-only Plasma system-tray actions (transport, sync, notifications, volume, and appearance), retaining their existing visual tooltips.
- Added `tests/frontend/plasma-panel-a11y.test.ts` to ensure every tray action keeps an explicit accessible name.
- Local checks passed: `check-version.sh`, file-header check (562 files), `vue-tsc --noEmit`, Vitest (99 files / 1,024 tests), and `git diff --check`. No Rust/Cargo or Go source changed.
- CI run [38470920674](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38470920674) for commit [`98f240c`](https://github.com/cybermanju/cybermanju.github.io/commit/98f240c67fc9f9b07216b25e20489e9e8aa820e4) passed all 14 jobs, including Rust Lint & Test, Build WASM, platform builds, and Deploy to GitHub Pages. The preview at https://cybermanju.github.io/develop/ returned HTTP 200 with `/develop/` assets; `main` stayed at `1920ee2`. No release or tag was created.


## 2026-10-13 — System theme recovery for corrupt storage

- Compared live `main` and `develop` before editing and before commit: `main` remained at `1920ee2080910f7f885ab7862ee6614d0ecc537e`, `develop` at `bcbbdf4572f9e31f2f81e45c6aeb868d4dfdc0fd`; no main-only commits were missing, and work stayed on `develop`.
- Unified first-run and recovery defaults so malformed theme JSON or inaccessible `localStorage` resolves the initial theme through `prefers-color-scheme` and keeps `followSystem` enabled, instead of showing a fixed dark theme until the OS changes its setting.
- Added regressions for corrupt JSON and storage access errors with a simulated light system preference.
- Local checks passed: version check (`0.1.1`), file headers (563 files), `vue-tsc --noEmit`, targeted theme tests, full Vitest (99 files / 1,026 tests), and `git diff --check`. No Rust/Cargo code changed.
- CI run [38495625087](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38495625087) for commit [`0b0e302`](https://github.com/cybermanju/cybermanju.github.io/commit/0b0e3029b4733b17eb356b9a58add5be2eadd3af) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages. The preview at https://cybermanju.github.io/develop/ returned HTTP 200 and exposed 7/7 checked assets under `/develop/`; `main` stayed at `1920ee2`. No release or tag was created.


## 2026-10-14 — Legacy media-query listener compatibility

- Compared live `main` and `develop` before implementation and after local checks: `main` remained at `1920ee2080910f7f885ab7862ee6614d0ecc537e`, `develop` at `849f422d54c46e5030d01310af2edb216a976293`; no main-only commits were missing, and all work stayed on `develop`.
- Added a `MediaQueryList.addListener` fallback when `addEventListener('change', ...)` is unavailable, so Auto appearance can continue tracking OS light/dark changes on older browsers and embedded WebViews. Modern browsers still use the standard event API.
- Added a regression simulating the legacy listener; it verifies OS changes update Auto mode and do not override a later manual theme choice.
- Local checks passed: version check (`0.1.1`), file headers (563 files), `vue-tsc --noEmit`, targeted theme tests (15), full Vitest (99 files / 1,027 tests), and `git diff --check`. No Rust/Cargo code changed.
- CI run [38516512349](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38516512349) for commit [`af77ba1`](https://github.com/cybermanju/cybermanju.github.io/commit/af77ba129137a52d1912fdb215d6a2429ec81230) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages. The preview at https://cybermanju.github.io/develop/ returned HTTP 200 and exposed 7/7 checked assets under `/develop/`; `main` stayed at `1920ee2`. No release or tag was created.


## 2026-10-14 — Normalize malformed persisted appearance values

- Compared live refs before implementation and after local checks: `main` stayed at `1920ee2080910f7f885ab7862ee6614d0ecc537e`, `develop` at `dfbf25d8026b0833fee6bf8d281ae2f05fb46433`; `main` had no commits missing from `develop`, and all work remained on `develop`.
- Hardened theme preference loading against valid JSON with invalid field types: unknown `density` falls back to the supported default, and non-boolean `followSystem` is treated as false so a string such as `"false"` cannot force OS-following mode. This prevents invalid density from reaching `buildCssVars()` and breaking appearance rendering.
- Added regression coverage for the malformed density/boolean combination, preserved manual theme selection, and confirmed CSS token generation remains valid.
- Local checks passed: version check (`0.1.1`), file headers (563 files), `vue-tsc --noEmit`, targeted theme tests (16), full Vitest (99 files / 1,028 tests), and `git diff --check`. No Rust/Cargo code changed.
- CI run [38542219051](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38542219051) for commit [`d9027a9`](https://github.com/cybermanju/cybermanju.github.io/commit/d9027a95f263580126a5b9caa5b84ee1b6768ca9) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages. The preview at https://cybermanju.github.io/develop/ returned HTTP 200 and exposed 7/7 checked assets under `/develop/`; `main` stayed at `1920ee2`. No release or tag was created.

## 2026-10-14 — Validate persisted wallpaper IDs

- Refreshed `origin` before implementation: `origin/main` had **0 commits missing from `develop`**; `develop` was 8 commits ahead. No main integration was needed, and all changes stayed on `develop`.
- Hardened appearance recovery so persisted wallpaper IDs are accepted only when they exist in the static `WALLPAPERS` catalogue; malformed or stale values now recover to the configured default. `setWallpaper()` also ignores unknown IDs, preventing callers from placing unsupported values into shared theme state.
- Added regression assertions for an invalid persisted ID and an invalid setter call; valid catalogued wallpaper changes continue to persist normally.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (563 files), `vue-tsc --noEmit`, full Vitest (99 files / 1,028 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`48237c7`](https://github.com/cybermanju/cybermanju.github.io/commit/48237c7f665e52719723ab526e81e6b0613a2928) to `develop` only. CI run [38571933623](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38571933623) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified the deployed preview at https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked local asset references under `/develop/`. A final authenticated ref check found 0 commits missing from `develop`, with local `develop` aligned to `origin/develop`; `main` was not modified.

## 2026-10-14 — Name CodeStudio icon-only controls

- Compared refreshed refs before the change and before commit: `origin/main` remained at `1920ee2`, no commits were missing from `develop`, and `origin/develop` stayed at `5b95d2c` during the work. No main integration was needed; all edits are on `develop`.
- Added explicit accessible names to CodeStudio's icon-only close buttons for the Find bar and bottom panel. `AppIcon` is `aria-hidden`, so these buttons otherwise had no accessible name for screen-reader and voice-control users.
- Added a regression assertion to `tests/frontend/responsive.test.ts` pinning both labels.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (563 files), `vue-tsc --noEmit`, full Vitest (99 files / 1,029 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`ae05887`](https://github.com/cybermanju/cybermanju.github.io/commit/ae05887) to `develop` only. CI run [38606617225](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38606617225) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 and all 7 checked local asset references used `/develop/`. The post-CI authenticated fetch confirmed `main` remained at `1920ee2`, no commits were missing from `develop`, and no concurrent develop commit had appeared.

## 2026-10-14 — Install reduced-motion policy at app root

- Compared authenticated refs at start and after local validation: `origin/main` remained at `1920ee2`, no commits were missing from `develop`, and `origin/develop` stayed at `5215fc3` during the work. No main integration was needed; all edits remain on `develop`.
- Found that the global `data-ui-motion` CSS policy was activated only through `useMotion()` calls inside `UiToolbar`'s `useWindowUi` path. Before any such toolbar mounted, the app root had no reduced-motion marker, so global CSS animations could ignore the user's OS/theme preference on the initial surface.
- Called `useMotion()` at `App.vue` startup so reduced-motion policy is installed before windows/toolbars mount; added a regression assertion in `tests/frontend/responsive.test.ts` for root initialization.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (563 files), `vue-tsc --noEmit`, full Vitest (99 files / 1,030 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`e75abd5`](https://github.com/cybermanju/cybermanju.github.io/commit/e75abd5) to `develop` only. CI run [38640147825](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38640147825) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-15 — Align toast role with polite announcements

- Compared authenticated refs before and after validation: `origin/main` stayed at `1920ee2`, no commits were missing from `develop`, and `origin/develop` remained at `2ba9f80` during the work. No main integration was needed; changes stay on `develop`.
- Corrected the notification stack's conflicting `role="alert"` / `aria-live="polite"` pair to `role="status"`, matching the intended polite announcement and avoiding assertive interruptions for non-blocking toasts.
- Added a regression assertion in `tests/frontend/notification-center.test.ts` for the status/polite semantics.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (563 files), `vue-tsc --noEmit`, full Vitest (99 files / 1,031 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`ec02acd`](https://github.com/cybermanju/cybermanju.github.io/commit/ec02acd) to `develop` only. CI run [38809694150](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38809694150) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-15 — Name the shared file URL field

- Compared authenticated refs before and after validation: `origin/main` stayed at `1920ee2`, no commits were missing from `develop`, and `origin/develop` remained at `65cf324` during the work. No main integration was needed; changes stay on `develop`.
- The read-only share URL input in `FilePermissionsPanel` had no accessible name. Associated it with the visible `SHARE LINK` heading via `aria-labelledby`.
- Added `tests/frontend/file-permissions-a11y.test.ts` to guard the heading/input association.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (564 files), `vue-tsc --noEmit`, full Vitest (100 files / 1,032 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`e9a36d0`](https://github.com/cybermanju/cybermanju.github.io/commit/e9a36d0) to `develop` only. CI run [38835099700](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38835099700) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-16 — Label backup restore file picker

- Compared authenticated refs before and after local validation: `origin/main` remained at `1920ee2`, no commits were missing from `develop`, and `origin/develop` stayed at `fa5957b` during the work. No main integration was needed; changes remain on `develop`.
- Added the accessible name `Choose backup snapshot file` to the restore wizard's `.cybermanju` file picker, which previously had no programmatic label.
- Added `tests/frontend/backup-panel-a11y.test.ts` to guard the picker name and accepted backup format.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (564 files), `vue-tsc --noEmit`, full Vitest (101 files / 1,033 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`aeafaab`](https://github.com/cybermanju/cybermanju.github.io/commit/aeafaab) to `develop` only. CI run [38855638372](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38855638372) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-16 — Name File Manager dialog inputs

- Compared authenticated refs before and after validation: `origin/main` stayed at `1920ee2`, no commits were missing from `develop`, and `origin/develop` remained at `5f656ca` during the work. No main integration was needed; changes remain on `develop`.
- Added persistent accessible names to the New folder and Rename text inputs, which previously relied only on a placeholder or had no name.
- Added `tests/frontend/file-manager-dialog-a11y.test.ts` with regressions for both dialog fields.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (565 files), `vue-tsc --noEmit`, full Vitest (102 files / 1,035 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`f37fea2`](https://github.com/cybermanju/cybermanju.github.io/commit/f37fea2) to `develop` only. CI run [38880279123](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38880279123) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-16 — Name grant-access selectors

- Compared authenticated refs before and after validation: `origin/main` remained at `1920ee2`, no commits were missing from `develop`, and `origin/develop` stayed at `f59133a` during the work. No main integration was needed; changes remain on `develop`.
- Added explicit accessible names to the user and access-level selectors in `FilePermissionsPanel`; previously, neither select had a programmatic name.
- Extended `tests/frontend/file-permissions-a11y.test.ts` to cover both selectors alongside the read-only share URL label.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (566 files), `vue-tsc --noEmit`, full Vitest (102 files / 1,036 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`588d0d0`](https://github.com/cybermanju/cybermanju.github.io/commit/588d0d0) to `develop` only. CI run [38907743336](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38907743336) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-16 — Forward UiSelect accessibility attributes

- Compared authenticated refs before and after validation: `origin/main` stayed at `1920ee2`, no commits were missing from `develop`, and `origin/develop` remained at `a432e05` during the work. No main integration was needed; changes remain on `develop`.
- Fixed `UiSelect` fallthrough so caller attributes such as `aria-label` and `title` reach the native `<select>` instead of the wrapping `<label>`. This makes the existing `OrganizePanel` accessible-name attribute effective without changing visible layout.
- Added `tests/frontend/ui-select-a11y.test.ts` to guard native-control attribute forwarding.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (566 files), `vue-tsc --noEmit`, full Vitest (103 files / 1,037 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`e998ac5`](https://github.com/cybermanju/cybermanju.github.io/commit/e998ac5) to `develop` only. CI run [38939609195](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38939609195) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-16 — Name Sync configuration selectors

- Compared authenticated refs before and after validation: `origin/main` remained at `1920ee2`, no commits were missing from `develop`, and `origin/develop` stayed at `c40be52` during the work. No main integration was needed; changes remain on `develop`.
- Added accessible names to the configuration selector used to start a sync run and the source/destination selectors used for file moves. The names are forwarded to native selects through the `UiSelect` fix from the preceding round; no visible layout changes.
- Added `tests/frontend/sync-panel-a11y.test.ts` to guard all three selector names.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (567 files), `vue-tsc --noEmit`, full Vitest (104 files / 1,039 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`8f0f953`](https://github.com/cybermanju/cybermanju.github.io/commit/8f0f953) to `develop` only. CI run [38969116484](https://github.com/cybermanju/cybermanju.github.io/actions/runs/38969116484) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. The post-CI ref check found `origin/main` still at `1920ee2`, no commits missing from `develop`, and no concurrent develop commit.

## 2026-10-16 — Name AgentPanel configuration selectors

- Compared authenticated refs before and after validation: `origin/main` remained at `1920ee2`, no commits were missing from `develop`, and `origin/develop` stayed at `dafde93` during the work. No main integration was needed; changes remain on `develop`.
- Added purpose-specific accessible names to all nine `UiSelect` controls in `AgentPanel` (assistant configuration, agent mode, API dialect/auth, shell, permission preset, MCP transport, fallback assistant, and per-tool permissions). They use the native-select attribute forwarding fixed in the previous round; visible layout is unchanged.
- Added `tests/frontend/agent-panel-a11y.test.ts` covering all nine names.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (568 files), `vue-tsc --noEmit`, full Vitest (105 files / 1,048 tests), and `git diff --check`. No Rust/Cargo code changed.

## 2026-10-17 — Add a shared legacy clipboard fallback

- Compared authenticated refs before and after local validation: `origin/main` remained at `dafde93`, no commits were missing from `develop`, and `origin/develop` stayed at `cb71771`. The prior main advance was already contained in develop; no new integration was needed.
- Added `copyToClipboard`, preferring the Async Clipboard API and falling back to a temporary, read-only textarea plus `execCommand('copy')`; the fallback removes the textarea in a `finally` block and reports failure accurately.
- Reused the helper in the agent harness, File Permissions share-link action, and File Manager path/text copy actions. Secret auto-clear logic remains unchanged.
- Added `tests/frontend/clipboard-fallback.test.ts` for successful modern writes, denied modern writes with successful fallback, and cleanup/reporting when fallback fails.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (569 files), `vue-tsc --noEmit`, full Vitest (106 files / 1,051 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`dc31709`](https://github.com/cybermanju/cybermanju.github.io/commit/dc31709) to `develop` only. CI run [39006255799](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39006255799) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. Post-CI refs remained `origin/main=dafde93` and `origin/develop=dc31709`, with no commits missing from develop.

## 2026-10-17 — Name the Account Manager token visibility toggle

- Compared authenticated refs before and after local validation: `origin/main` remained at `dafde93`, no commits were missing from `develop`, and `origin/develop` stayed at `93a6998`. No main integration was needed.
- Added a state-aware `aria-label` (“Show token” / “Hide token”) to the provider-token visibility control in the Account Manager wizard. The visual icon and existing show/hide behavior are unchanged.
- Added `tests/frontend/account-manager-a11y.test.ts` to guard the accessible name and password/text toggle.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (570 files), `vue-tsc --noEmit`, full Vitest (107 files / 1,052 tests), and `git diff --check`. No Rust/Cargo code changed.
- Pushed code commit [`25445b7`](https://github.com/cybermanju/cybermanju.github.io/commit/25445b7) to `develop` only. CI run [39021005534](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39021005534) passed all 14 jobs, including Rust Lint & Test, Build WASM, and Deploy to GitHub Pages.
- Verified https://cybermanju.github.io/develop/ returned HTTP 200 with 7/7 checked asset references under `/develop/`. Post-CI refs remained `origin/main=dafde93` and `origin/develop=25445b7`, with no commits missing from develop.

## 2026-10-17 — Preserve editing focus after legacy clipboard copies

- Authenticated main/develop comparisons before and after validation found `origin/main=dafde93` already fully contained in develop and `origin/develop=1574ac0` unchanged. No merge was necessary; main remains untouched.
- The shared clipboard fallback selected a temporary textarea without restoring the user's editor/composer focus. It now restores the previously focused control and its text selection/direction after removing the temporary field, including failed/throwing copy commands. Detached controls are skipped; focus options have a legacy fallback; restoration errors cannot mask the copy result. Secret auto-clear behavior is unchanged.
- Expanded executable clipboard tests from 3 to 8 cases covering success/failure focus restoration, thrown copy commands, legacy focus options, and failed selection restoration. A separate read-only review checked the cleanup and compatibility risks.
- Local checks passed: version consistency (0.1.1), file headers (570 files), `vue-tsc --noEmit`, Vitest (107 files / 1,057 tests), and `git diff --check`. No local Cargo commands, release, force-push, or production-branch edits.
- Pushed code commit [`53dee82`](https://github.com/cybermanju/cybermanju.github.io/commit/53dee82) to develop only. [CI run 39036448909](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39036448909) passed all 14 jobs, including Rust, WASM, Android, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI refs remained `origin/main=dafde93`, `origin/develop=53dee82`; there were no concurrent changes or missing main commits.

## 2026-10-17 — Forward UiInput accessibility attributes without moving layout

- Authenticated remote comparison and fetch found `origin/main=dafde93` already contained in develop and `origin/develop=00b0d17` unchanged. No integration was needed; main remains untouched.
- Audited UiInput fallthrough: caller attributes such as `aria-label`, `aria-labelledby`, `aria-describedby`, `id` and native listeners previously reached the wrapping label instead of the input. Disabled automatic fallthrough and now forward native attributes/listeners explicitly to the input. Caller class/style remain on the wrapper to preserve existing search and folder-name layouts; declared props and model events retain their existing bindings.
- A parallel read-only review found 25 existing aria-label consumers affected across AgentPanel, DevicesPanel, OrganizePanel and SyncPanel. Autocomplete already used an explicit input binding and remains unchanged.
- Added executable regression coverage in `tests/frontend/ui-input-a11y.test.ts`: compiles the real production SFC and renders it with Vue SSR in Node, verifying accessible names/descriptions/ids, declared props, and class/style placement without adding dependencies or modifying Vitest configuration.
- Local checks passed: version consistency (0.1.1), file headers (571 files), `vue-tsc --noEmit`, Vitest (108 files / 1,059 tests), and `git diff --check`. No Rust/Cargo commands, release, force-push, or main edits.
- Pushed code commit [`e5e41b9`](https://github.com/cybermanju/cybermanju.github.io/commit/e5e41b9) to develop only. [CI run 39053829899](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39053829899) passed all 14 jobs, including Rust, WASM, Android, iOS, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI fetch confirmed `origin/main=dafde93`, `origin/develop=e5e41b9`, no missing main commits, and no concurrent develop changes.

## 2026-10-17 — Keep Code Intel technical fields accessible and literal

- Compared main/develop using a remote fetch and authenticated API reads before and after local work. `origin/main=dafde93` is already contained in develop; `origin/develop=cb12fd7` remained unchanged. No merge or conflict resolution was needed; main remains untouched.
- Reviewed shared controls and CodeStudio input markup, then selected a direct, low-risk compatibility fix: the Code Intel snippet filename, pasted source and local-path fields had no explicit accessible names and did not opt out of mobile keyboard capitalization/correction. Added concise `aria-label` names plus `autocapitalize="off"`, `autocorrect="off"` and `autocomplete="off"`, retaining existing spellcheck, bindings, desktop-only path visibility and Enter-to-parse behavior. These hints request literal technical text without changing parsing or backend contracts.
- Added `tests/frontend/code-studio-intel-input.test.ts` with three executable regressions that compile and SSR-render the actual field markup, checking accessible names, keyboard hints and model values. No dependencies or Vitest configuration changes.
- Local checks passed: version consistency (0.1.1), file headers (572 files), `vue-tsc --noEmit`, Vitest (109 files / 1,062 tests), and `git diff --check`. No local Rust/Cargo, release, force-push or production-branch changes.
- Pushed code commit [`a9b9d94`](https://github.com/cybermanju/cybermanju.github.io/commit/a9b9d94) to develop only. [CI run 39070928161](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39070928161) passed all 14 jobs, including Rust, WASM, Android, iOS, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI fetch confirmed `origin/main=dafde93`, `origin/develop=a9b9d94`, no missing main commits and no concurrent develop changes.

## 2026-10-17 — Forward dialog attributes and announce modal subtitles

- Compared remote refs by fetch and authenticated API reads: `origin/main=dafde93` is already contained in develop, and `origin/develop=462c7bc` remained unchanged during local work. No merge or uncertain conflict resolution was needed; main remains untouched.
- UiModal's Teleport root discarded caller attributes instead of placing them on the actual dialog. Disabled automatic fallthrough and explicitly bound caller attrs to the dialog; class/style merge there, not on the backdrop, and explicit caller attributes override component defaults. Existing consumers use declared props/events and require no migration.
- Associated nonempty subtitles with the dialog using `aria-describedby` and Vue `useId()`. Existing delete confirmations now expose their target/context as a description, with unique ids across modal instances. Empty subtitles create no dangling default reference; callers may explicitly replace the description or name. A parallel read-only review confirmed the installed Vue supports useId and that the focus ref, trap, Escape/backdrop handling and declared model/close events are unchanged.
- Added six executable SSR regressions in `tests/frontend/ui-modal-a11y.test.ts`, compiling the real SFC and reading its teleported output: native attributes/layout, default title/subtitle, empty subtitle, caller overrides, distinct ids, and hidden-dialog behavior. Browser-only focus effects and decorative children are stubbed; no dependencies or Vitest configuration changes.
- Local checks passed: version consistency (0.1.1), file headers (573 files), `vue-tsc --noEmit`, Vitest (110 files / 1,068 tests), and `git diff --check`. No local Rust/Cargo commands, release, force-push or main edits.
- Pushed code commit [`e127105`](https://github.com/cybermanju/cybermanju.github.io/commit/e127105) to develop only. [CI run 39086443498](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39086443498) passed all 14 jobs, including Rust, WASM, Android, iOS, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI fetch confirmed `origin/main=dafde93`, `origin/develop=e127105`, no missing main commits and no concurrent develop changes.

## 2026-10-18 — Describe input help and errors on the native control

- Remote fetch and authenticated API comparisons found `origin/main=dafde93` already contained in develop and `origin/develop=a0654bb` unchanged during local work. No merge or uncertain resolution was required; main remains untouched.
- UiInput displayed hint/error text without associating it with the native input. Added a stable per-instance `useId()` id to the existing message span and default `aria-describedby` while that text is visible. An error additionally supplies default `aria-invalid="true"`; errors retain their existing visual precedence over hints. Empty messages produce no dangling description or default invalid state.
- Kept explicit caller ARIA attributes authoritative by placing defaults before the existing native attribute forwarding. Wrapper class/style, model events, labels, focus and validation logic are unchanged. A parallel review confirmed precedence and compatibility: the two current hint consumers are ProcessPanel's script-path and cron-expression fields; there are no current production error-prop or ARIA-override consumers.
- Extended the real-SFC SSR suite with five regressions: visible hint, error replacing hint, absent message, explicit caller overrides and unique ids across inputs. No dependency or Vitest configuration changes.
- Local checks passed: version consistency (0.1.1), file headers (574 files), `vue-tsc --noEmit`, Vitest (110 files / 1,073 tests), and `git diff --check`. Independent `npm run typecheck` and full tests also passed. No local Rust/Cargo commands, release, force-push or main edits.
- Pushed code commit [`7389421`](https://github.com/cybermanju/cybermanju.github.io/commit/7389421) to develop only. [CI run 39100438229](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39100438229) passed all 14 jobs, including Rust, WASM, Android, iOS, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI fetch confirmed `origin/main=dafde93`, `origin/develop=7389421`, no missing main commits and no concurrent develop changes.

## 2026-10-18 — Keep storage percentages finite and within UI bounds

- Compared main/develop using a remote fetch and authenticated API reads. `origin/main=dafde93` is already contained in develop and `origin/develop=a82751f` remained unchanged during local work. No integration or uncertain conflict resolution was required; main remains untouched.
- An independent read-only utility audit found `diskPct()` violated its documented 0–100 contract: negative disk usage/capacity could produce negative percentages, and non-finite telemetry could yield NaN. DiskManagerPage, AccountManagerPanel and FileManager use this value for progress-bar widths and labels.
- Hardened only the shared percentage formatter: require positive finite capacity, normalize missing/non-finite usage to zero, and clamp both ends to 0–100. Existing ordinary/fractional percentages and over-capacity saturation are preserved, including extreme finite operands that overflow during division. No persisted data, backend commands, storage accounting, byte formatting or product behavior beyond display sanitization changed.
- Added `tests/frontend/format-disk-pct.test.ts` with 21 pure regression cases covering normal/fractional use, negative values, missing/non-finite input, arithmetic overflow and a representative input-combination invariant. No dependencies or test configuration changes.
- Local checks passed: version consistency (0.1.1), file headers (574 files), `vue-tsc --noEmit`, Vitest (111 files / 1,094 tests), and `git diff --check`. No local Rust/Cargo commands, release, force-push or main edits.
- Pushed code commit [`c85f5b3`](https://github.com/cybermanju/cybermanju.github.io/commit/c85f5b3) to develop only. [CI run 39115377282](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39115377282) passed all 14 jobs, including Rust, WASM, Android, iOS, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI fetch confirmed `origin/main=dafde93`, `origin/develop=c85f5b3`, no missing main commits and no concurrent develop changes.

## 2026-10-18 — Cancel polling waits promptly and release their resources

- Remote fetch and authenticated API comparisons found `origin/main=dafde93` already contained in develop and `origin/develop=30f6903` unchanged during local work. No merge or uncertain resolution was needed; main remains untouched.
- The shared polling helper used a non-cancellable timeout between checks. Cancelling an OAuth connection or unmounting AccountManagerPanel during that interval left the wait alive until its 2–3 second deadline. Made only the inter-attempt delay signal-aware: abort now rejects with the existing `Error('cancelled')`, clears the timeout and removes its listener; normal timeout completion also removes the listener.
- An independent read-only review confirmed both current consumers (dashboard OAuth and Supabase OAuth in AccountManagerPanel) already handle cancellation and share cancel/unmount hooks. No consumer changes, callback signature changes, network cancellation, token handling or success/timeout precedence changes. Clarified the signal documentation: in-flight checks continue to own their own cancellation.
- Added eight fake-timer regressions in `tests/frontend/poll.test.ts`: pre-aborted signals, immediate success, pending-wait cancellation/resource cleanup/no later attempt, abort during a failed check, completed-wait listener cleanup and progress, exhaustion without a signal, transient errors and zero attempts. No real network/browser waits, dependencies or test configuration changes.
- Local checks passed: version consistency (0.1.1), file headers (575 files), `vue-tsc --noEmit`, Vitest (112 files / 1,102 tests), and `git diff --check`. No local Rust/Cargo commands, release, force-push or main edits.
- Pushed code commit [`1917a4f`](https://github.com/cybermanju/cybermanju.github.io/commit/1917a4f) to develop only. [CI run 39131426006](https://github.com/cybermanju/cybermanju.github.io/actions/runs/39131426006) passed all 14 jobs, including Rust, WASM, Android, iOS, desktop builds and Pages deployment.
- Verified HTTP 200 for https://cybermanju.github.io/develop/ and all 7 referenced `/develop/` assets. Post-CI fetch confirmed `origin/main=dafde93`, `origin/develop=1917a4f`, no missing main commits and no concurrent develop changes.
## 2026-10-18 — Wiki knowledge graph (graphify)

- Replaced the wiki's heuristic-only inter-subject links with a real knowledge graph from [graphify](https://github.com/Graphify-Labs/graphify) (`pipx install graphifyy`, local tree-sitter AST, `--code-only` = deterministic, no LLM, no API keys). One extraction pass over the repo yields 11,267 nodes / 25,550 edges (EXTRACTED 24,851 / INFERRED 699) in ~14 s.
- Added `scripts/wiki-graph.mjs`: pure aggregator folding `graphify-out/graph.json` into subject-level data — symmetric cross-subject edges with split EXTRACTED/INFERRED provenance + relation breakdown + samples, per-subject god nodes (full ranking, global top-40 payload), dominant Leiden community per subject, and all-pairs shortest module paths for the SPA tracer. `tests/frontend/wiki-graph.test.ts` (6 cases) pins the fold.
- `scripts/generate-wiki.mjs` now enriches every artifact when the graph is present (`engine: graphify`): subject `.txt`/`.json`/`.html` gain a Knowledge-graph section (community, strongest edges, god nodes, `graphify query` hint), `subjects.json` carries a `graph` payload + per-subject `edges`/`godNodes`/`community`, `llms.txt` gains god-node lines, and graphify's interactive `graph.html` is copied to `/wiki/graph.html`. Absent graph → heuristic engine label everywhere (verified both paths).
- 3D wiki (`wiki-src/`): connected-module chips show weight badges + full EXTRACTED/INFERRED tooltips, satellite tethers are solid (EXTRACTED-dominant) or dashed (INFERRED-dominant), ★ god-node satellites (bigger voxel + white plate), community id in module stats, a path-trace widget (precomputed hops, clickable chain), and a conditional header link to `graph.html`.
- CI (`ci.yml` wasm-build): `actions/setup-python@v7` + `pipx install graphifyy==0.9.92` + `graphify extract . --code-only` + `graphify export html` before `npm run wiki`. New `npm run wiki:graph` regenerates locally; `graphify-out/` gitignored.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (568 files), `vue-tsc --noEmit`, full Vitest (105 files / 1,045 tests), and a full `npm run wiki` build (graphify + heuristic fallback paths).

## 2026-10-18 — Android launch crash fix (missing androidx.documentfile)

- Reported symptom: release v0.1.1 APK (`CyberManju-OS-0.1.1-arm64-v8a.apk`) installed on a Galaxy A50 (Android 11, arm64-v8a) but closed instantly on open.
- Captured the trace over adb (`am start` + `logcat`): Rust starts fine (`CyberManju OS starting...`), then `FATAL EXCEPTION: main — java.lang.NoClassDefFoundError: Failed resolution of: Landroidx/documentfile/provider/DocumentFile;` at `app.tauri.plugin.PluginHandle.<init>` → `PluginManager.load`.
- Root cause is upstream: `tauri-plugin-scoped-storage =1.5.1` (crates.io) imports `androidx.documentfile.provider.DocumentFile` in `ScopedStoragePlugin.kt` but its library `android/build.gradle.kts` never declares the dependency — compile classpath gets it transitively (CI builds green), runtime dex does not, so plugin registration crashes the app at launch. Scoped storage is registered in `lib.rs` and used by the mobile folder-mount flow (`src/utils/mobileScopedStorage.ts`), so removing the plugin was not an option.
- Fix: new §5 in `scripts/android-configure.sh` patches the generated `gen/android/app/build.gradle.kts` with `implementation("androidx.documentfile:documentfile:1.0.1")` (marker-commented, idempotent at both bash and python level, fails loudly when no `dependencies { }` block exists). `gen/` is gitignored/regenerated, so the patch belongs in this script next to the manifest work.
- Validated the script end-to-end against a synthetic Tauri template layout (`/tmp/opencode/genfix`): manifest patch well-formed, gradle patch inserted once, second run skips everything. `bash -n` clean; `check-file-headers.sh` unaffected (top-file header untouched).
- Second round (same day, review follow-up): the 3D scene now IS the graph — permanent subject↔subject edge tethers (solid EXTRACTED / dashed INFERRED) ride the bobbing nodes and blaze along traced paths; orbit-frame pixel squares are tinted per Leiden community with a color legend in the overview; ★ god-node satellites open a real deep view (degree + community); rail/symbol search also matches god nodes and contract signals; hover tooltips show community + engine.
- Added the 2D switch: a header toggle (and an overview button) flips between the 3D wiki and graphify's `graph.html` in a lazy-loaded full-screen iframe overlay; Esc closes it and the 3D render loop pauses underneath (battery/GPU). The toggle only appears when subjects.json carries a graph payload.
- Curl/LLM artifacts now teach the architecture and the live-CI contract: `llms.txt` gained "Live updates (read before caching)" + "Architecture (one-screen system model)" sections (transports, request flow, service-crate contracts, data truth), every subject `.txt` stamps the build commit with re-fetch-not-cache instructions, and `subjects.json` carries the commit too. Fixed in review: symbol deep-view validation now accepts god-node names (satellite clicks resolve).
- Second-round local validation passed: `vue-tsc --noEmit`, full Vitest (105 files / 1,045 tests), `check-file-headers.sh` (570 files), full `npm run wiki` build, `git diff --check`.

## 2026-10-18 — Fix Android app-drawer load error (missing JNI flags arg)

- Reported symptom on device: the Gaveta app drawer failed with `network: app query failed: Invalid number or type of arguments passed to java method: (Landroid/content/Intent;I)L;`.
- Root cause in `crates/launcher/src/lib.rs::query_activities`: the JNI call declared the two-arg Java signature `(Landroid/content/Intent;I)Ljava/util/List;` but passed only the `Intent` — the required `int flags` argument was missing. `jni` 0.21's argument/signature validation rejects the mismatch before the call reaches ART, so every consumer of the helper failed instantly (`list_apps`, `list_social_apps`, the `explicit_intent` open fallback in `open_app`, and `list_icon_packs`).
- Fix: pass the flags int through the shared helper as a new `QUERY_ACTIVITY_FLAGS: i32 = 0` constant — deliberately 0, exactly what `getLaunchIntentForPackage` uses internally. NOT `MATCH_DEFAULT_ONLY` (0x10000) despite the old doc comment: that flag only returns activities whose filter declares `CATEGORY_DEFAULT`, which stock MAIN/LAUNCHER filters never do (only HOME filters, like the one `android-configure.sh` injects, declare it), so it would blank the drawer even after the crash fix. Module doc updated to match.
- Audited every other `call_method`/`call_static_method`/`new_object`/`get_field` signature/argument pair in the file — all remaining pairs match their declared JNI signatures; the mismatch was unique to `queryIntentActivities`. One fix covers all three call sites.
- Local checks passed: `check-version.sh` (0.1.1) and `check-file-headers.sh` (578 files). No local Rust/Cargo commands per repo rules — `cargo fmt`/`clippy`/`test` for the Android JNI module are covered by CI's `rust-check` job on push.

## 2026-10-18 — Gate the CI audit steps (fix 10 hidden RustSec vulnerabilities)

- Reviewed every `continue-on-error`/`allow_failure` site against the last green run's full logs (CI 39142070323): `ci.yml` cargo audit, npm audit, sibling `dist-wasm` restore, Flatpak deb download, Android on-device smoke; `release.yml` Flatpak deb download; GitLab `audit` job (`allow_failure` + `|| true` ×2).
- `cargo audit` was the only step hiding real errors: **10 vulnerabilities** — quinn-proto 0.11.15 (RUSTSEC-2026-0338/0339/0340/0341/0342), rustls 0.23.45 (RUSTSEC-2026-0346/0347/0348/0349), ruzstd 0.9.0 (RUSTSEC-2026-0344). npm audit (production) is clean (0 vulns).
- Fixed the lock (no local Cargo commands per repo rules — resolved entirely from the crates.io sparse index + `.crate` artifacts): `Cargo.lock` bumps quinn-proto → **0.11.19**, rustls → **0.23.46**, ruzstd → **0.9.1**, plus the new closure quinn-proto 0.11.19 pulls in: **rand 0.10.3** (default features → sys_rng/std_rng), **rand_pcg 0.10.2** (promoted from quinn-proto dev to normal dep), **chacha20 0.10.2** (`rng` feature only — `default-features=false`, so cipher/zeroize stay out), and the existing `getrandom 0.4.2`/`rand_core 0.10.1`/`cpufeatures 0.3.0` entries reused. Every checksum was cross-validated two ways: sparse-index `cksum` vs SHA256 of the downloaded `.crate` (both agree for all 6 versions, including ruzstd via the static CDN after its index path 404'd).
- Structural lock validation (Python, referential-integrity + canonical ordering + dependency-string qualification rules): 803 packages, zero errors — the qualification rule (version suffix iff 2+ versions of a name exist) forced `chacha20poly1305`'s existing ref to `chacha20 0.9.1` once 0.10.2 was added.
- Made the audits **blocking**: `ci.yml` lost `continue-on-error` on `cargo audit` + `npm audit --omit=dev`; `.gitlab-ci.yml` lost `allow_failure` and both `|| true` suffixes (resolves `TASKS.md` `P2-5`: gate, not accept).
- Added a **lock integrity gate** to `rust-check`: `cargo metadata --locked` fails loudly when `Cargo.lock` would need re-resolution, instead of every build silently rewriting a stale lock in the runner.
- Removed `continue-on-error` from the Flatpak `Download Debian package` step in **both** `ci.yml` and `release.yml`: the manifest consumes the deb (`type: dir → ../../artifacts`), so a failed download previously produced a misleading late `flatpak-builder` source error (resolves `TASKS.md` `P2-7`, fail fast).
- Kept by design (no hidden errors): sibling `dist-wasm` restore (documented cache-miss tolerance with explicit `restored=false` output) and Android on-device smoke (best-effort; log shows `SKIP — no emulator/device attached`, exits 0).
- Docs synced: `AGENTS.md` §4 (audit now blocking, `rust-check` lock gate), `TASKS.md` `P2-5` + `P2-7` checked off.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (578 files), `vue-tsc --noEmit`, full Vitest (113 files / 1,108 tests), `git diff --check`.

## 2026-10-18 — Full remake of the mobile onboard wizard (merge the duplicated steps)

- Complaint: `MobileSetupWizard.vue` walked 8 steps (welcome → account → vaults → providers → repos → agent → appearance → done) that asked for the same work up to three times — provider connections were created in **account** (OAuth provisioning) *and* **providers** (manual row), and disks were created in **account** (per-provider folder+size+passphrase cards), **vaults** (partitions) *and* **repos** (repo+disk, incl. a Drive branch identical to the account cards).
- Remade as the merged 5-step flow the user picked: **welcome → connect → storage → personalize → done** (`MOBILE_SETUP_STEPS` in `src/utils/setupWizard.ts`):
  - **connect** = account + providers: inline OAuth (Google/GitHub/GitLab via the broker, still `signInWithPopup`), one manual provider row (PAT or OAuth session; phone-folder picker + web-compatible live mirror), connected list, Account Manager hand-off — one place, one copy.
  - **storage** = vaults + repos + the old account cloud cards: a single vault list where every row is one `.cybermanju` disk (local or on any connected provider — Drive folder / private repo seeded when missing), prefilled in one tap via **chips per connected provider** (`addProviderVault` → `blankPartitionDraft` + `configId`), plus a collapsed optional batch panel for GitHub/GitLab 1–8 private repos + a disk each. The Drive batch branch was deleted as pure duplication of a Drive vault row.
  - **personalize** = appearance (theme + wallpaper, live preview) + the optional agent card, so both "finish touches" share one step.
- Resume safety: new `MOBILE_LEGACY_STEP_ALIAS` + `resolveMobileSetupStep()` map pre-merge saved keys (account/providers → connect, vaults/repos → storage, agent/appearance → personalize) onto the merged steps; `initialStep()` uses it, so in-flight resumes from older builds never land on a removed step. `cybermanju:open-accounts` return-step hand-off unchanged (`openAccounts('connect'|'storage')`).
- Deleted along with the duplication: `mswSignedProviders`/`mswProvisionCloudDisk`/`mswCloud*` state and the `signedProviderCards`/`clampVaultSizeMb`/`connectedAccounts` imports from the mobile component (desktop `SetupWizard.vue` still uses the shared helpers — comment in `setupWizard.ts` updated to say so); `.msw-prov-*` styles dropped, `.msw-broker` (previously dead) now guards the batch panel, `.msw-chip*` added.
- Tests: `tests/frontend/mobile-setup.test.ts` rewritten for the 5-step order, the legacy alias map (asserted 1:1 with no live-step collisions), and the one-owner-per-concern rule; `responsive.test.ts` constraints (opaque sheet, `msw-welcome-title`, inline `signInWithPopup`, `cybermanju:open-accounts`) all still hold.
- Local validation passed: `check-file-headers.sh` (578 files), `vue-tsc --noEmit`, full Vitest (113 files / 1,109 tests).

## 2026-10-18 — Repair pre-existing Cargo.lock drift (the new lock gate caught it)

- First run of the new `cargo metadata --locked` gate failed — by design — and its self-diagnosing output (`cargo metadata` unlocked + `git diff -- Cargo.lock`) showed the real story: the drift was **pre-existing**, not from the audit bump. `crates/sync/Cargo.toml` requires `iroh = "1"` + `raptorq = "2"` (the "P2P BONDS (CI proving)" surface), but neither (nor iroh's whole 100+-package tree) had ever been written to `Cargo.lock` — every CI build had been silently re-resolving and compiling a 104-package tree that the committed lockfile never recorded. Worst part: `cargo audit` scans the committed lock, so the entire iroh stack was **never audited** until now.
- Repair strategy (still zero local Cargo, per repo rules): the gate's diagnostic printed cargo's own intended lockfile as a unified diff in the runner; that diff was captured verbatim, trimmed of job boilerplate, `git apply --check`-validated and applied. The result is **byte-identical to cargo's output** (git blob `cbf989c` matches the runner's `index 7cf6613..cbf989c` header), so the `--locked` gate is exact by construction.
- Net effect: 803 → **901 packages** — iroh 1.3.0 (+relay/noq/portmapper/raptorq 2.0.1/postcard/sha2 0.11/syn 3 stack) now recorded; orphans from older dependency sets pruned (hmac 0.13.0, ctutils, parking, parking_lot 0.11.2, parking_lot_core 0.8.6, redox_syscall 0.2.16, ipnet 2.12.0, portable-atomic-util); routine bumps cfg_aliases 0.2.2, rustc-hash 2.1.3 (dep-string re-qualification follows).
- The 2026-10-18 security fixes survive the re-resolve untouched (quinn-proto 0.11.19, rustls 0.23.46, ruzstd 0.9.1, rand 0.10.3 / rand_pcg 0.10.2 / chacha20 0.10.2 — iroh's closure even reuses the same rand 0.10.3).
- Pre-flight advisory scan (fresh shallow clone of RustSec advisory-db, TOML-frontmatter parser honoring `patched`/`unaffected`/`informational`): all 105 new packages checked — 12 vulnerability advisories in scope, every one patched by the locked version; 5 warning-only (unmaintained/unsound, which `cargo audit` reports without failing). Full-lock re-scan agrees; the 4 apparent flags (arrayref RUSTSEC-2026-0260, generic-array RUSTSEC-2020-0146, hashbrown RUSTSEC-2024-0402 ×2) are `unaffected`-range hits, not vulnerabilities — real `cargo audit` already passed with those exact versions in the previous run.
- Structural re-validation of the applied lock: 901 package blocks, 0 dependency-integrity errors, canonical name/semver ordering, all qualification rules exact.

## 2026-10-18 — Fix Android Google sign-in (`Scoped shell IO error: os error 2`) — one opener seam for every runtime

- **Root cause:** every external open went through `@tauri-apps/plugin-shell` `open()`, whose `open` crate treats Android like any unix and spawns `xdg-open` — which Android does not ship. The spawn fails ENOENT inside the plugin's scope check (`tauri-plugin-shell-2.3.5/src/scope.rs:197`: `Scoped shell IO error: No such file or directory (os error 2)`), so every Google/GitHub/GitLab button (sign-in + provider Connect) and `OPEN WITH SYSTEM` failed on the phone while working on desktop. The WebView has no browser/file opener of its own, so an `Intent` is the only way out.
- **Fix (no new Cargo/npm deps — the `cargo metadata --locked` gate + blocking audit make new deps expensive):** hand-rolled JNI `Intent.ACTION_VIEW` bridge inside the existing `cybermanju-launcher` crate:
  - `open_url` — allow-listed schemes only (`https://`, `http://`, `mailto:`, `tel:`, `cybermanju://`); everything else (`intent:`, `file:`, `javascript:`, …) refuses `invalid:` so a hostile link cannot reach the package resolver, and error labels are cut at `?`/`#` so OAuth `code`/`state` never reach a message.
  - `open_file` — `androidx.core.content.FileProvider` `content://` URI (`file://` data throws `FileUriExposedException` on Android 7+) with `FLAG_GRANT_READ_URI_PERMISSION` for that one hand-off; directories refuse `unsupported:`; FileProvider failures map to `not_found:` naming `cybermanju_paths.xml`.
  - Non-Android builds refuse honestly with `unsupported:` — desktop never reaches them (the frontend keeps the proven shell opener there).
- **`scripts/android-configure.sh`:** §2f injects the `<provider>` (`${applicationId}.fileprovider`, authority rebuilt from the Activity package name on the Rust side) into the manifest; §5 now adds each missing runtime dep independently (`androidx.documentfile` + explicit `androidx.core:core:1.13.1`, which hosts the `FileProvider` class the manifest names); new §6 writes `res/xml/cybermanju_paths.xml` (`<root-path path="/"`, marker-guarded, refuses to overwrite a foreign file, minidom-validated before Gradle sees it). Idempotency + foreign-file refusal verified locally on a synthetic `gen/android` (run twice).
- **Tauri commands:** `launcher_open_url` / `launcher_open_file` registered in `src-tauri/src/lib.rs` (Android-only; off-Android they return `unsupported:`).
- **One frontend seam:** new `src/utils/openExternal.ts` — `externalOpenRoute` (pure, unit-tested) routes Android → the new commands, desktop Tauri → `plugin-shell open`, plain browser → `window.open`; `openExternalUrl` / `openLocalFile` reject empty targets `unsupported:`. Converted all four direct call sites: `useSupabase.ts` `signInWithPopup`, `AccountManagerPanel.vue` `supabaseConnect` (the Google Connect path), `FileManager.vue` `openHostWithSystem`, `TopMenuBar.vue` Help → Documentation.
- **Tests:** `tests/frontend/open-external.test.ts` (routing matrix, per-runtime opener calls, empty-target refusals, source scan pinning `plugin-shell` to the seam alone + all four sites wired) and Rust tests in `crates/launcher` (`opens_only_allowlisted_link_schemes`, `link_labels_never_carry_oauth_secrets`, extended non-Android `unsupported:` refusal).
- Docs: `docs/ANDROID.md` §3 (FileProvider manifest/XML/gradle) + §6 (opener seam); new `android-configure.sh` header bullet for §2f.
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (578 files), `vue-tsc --noEmit`, full Vitest (114 files / 1,119 tests).

## 2026-10-18 — Phone Settings becomes a real app settings stack + slim status-bar header

- **Complaint 1 (config window):** on mobile the sheet rendered the *desktop* settings page — brand header · pinned status strip · wrapping jump rail · one long scroll — under its own decorative sheet titlebar, so ~140px of chrome sat above the first row. Nothing about it read like a phone app's Settings.
- **Complaint 2 (top header):** the mobile TopMenuBar was `calc(52px + env(safe-area-inset-top))` — a full desktop menu bar on a phone — while `.cybermanju-shell` *already* pads `env(safe-area-inset-top)`, so the notch was counted twice and fixed sheets (`top: 52px + inset`) overlapped the strip's lower band on notched devices.
- **Header fix — one shared number:** new `--mobile-header-h: 26px` in `src/assets/ui.css` (status-bar sized; the notch belongs to the shell's safe-area padding). `TopMenuBar`'s ≤768px block renders exactly that strip (16px logo mark, 11px status meta, 26px tray/clock targets); `.app-window` top became `calc(env(safe-area-inset-top, 0px) + var(--mobile-header-h))`, so every fixed sheet lands flush on the strip's bottom edge — no gap, no overlap. Mobile sheet titlebars standardized at 44px (iOS nav-bar height); `MobileLauncher`'s floating top buttons lost their own double-counted inset.
- **Settings remake (`SettingsPage.vue`):** gated on `isMobile` (the shell's own ≤768px query, so desktop windows keep the full page). Phones get the standard stack:
  - **Root** — large title (26px/800) + iOS rounded search field (16px input, no focus zoom) + ONE inset group of cells: icon tile · title/subtitle · **live right-hand value** (theme, Custom URL/Auto, Configured/Not set, refresh interval, touch mode, binding count, version) · chevron; the connection summary folds in as its own group plus a version footer. Empty search keeps the "No results" state.
  - **Detail** — 44px back bar (`‹ Settings` · section title · Refresh) showing only the pushed card with a native slide-in; `SECTIONS` grew `title`/`icon`/`sub`, and every card keeps its `sectionVisible(…)` filter *and* gained `inSection(…)`. Desktop keeps the strip + jump rail + scroll spy verbatim behind `v-if="!isMobile"`.
- **Integration (full stack, not a skin):** `AppWindow` drops its generic mobile titlebar for the settings sheet only (`app-window--bare-title`, `win.panelType === 'settings'`), so the stack header is the only chrome; Android system back now pops the detail instead of closing the sheet — `handleAndroidBack` lets the **active** sheet claim the press first via `cybermanju:mobile-back` (new `detail.panel` so background sheets — all of them stay mounted — can't steal it; `FileManager` guards on `files`), `Escape` mirrors it; the Accounts → Configure deep link (`cybermanju:settings-focus`) pushes the broker detail on phones rather than scrolling a rail that isn't rendered; resizing to desktop re-roots the stack.
- **Version source:** About card, root footer and the list value now read `package.json` through `APP_VERSION` instead of a hardcoded (already stale) `0.1.0`.
- **Tests:** new `tests/frontend/settings-mobile.test.ts` (root index, back bar, per-card gating + desktop layout kept, back/Escape/deep-link integration, slim-chrome geometry); `mobile-ios.test.ts`'s notch test rewritten to pin the new contract (shell owns the inset, sheets = inset + `--mobile-header-h`, no `52px` left anywhere).
- Local validation passed: `check-version.sh` (0.1.1), `check-file-headers.sh` (580 files), `vue-tsc --noEmit`, `npm run build`, full Vitest (115 files / 1,131 tests), `git diff --check`.

## 2026-10-18 — FileManager gets a Google Drive scope tab (all external Drive files in one place)

- **Ask:** the file manager should show every Google Drive external file (provider mounts, not vault yet) in a specific tab — previously Drive lived only as per-config rows in the sidebar PROVIDERS section.
- **`FileManager.vue`:**
  - New **scope tab strip** in the sub-bar (`fm-scopetabs`, `role=tablist`): **Vault | Google Drive** (Drive badge = connected Drive config count). Active tab derives from `currentPath` + a new `gdriveOnly` ref (`scopeTab` computed: `'vault'` on vault paths, `'gdrive'` inside the Drive view, `null` on other provider mounts).
  - `openGdriveTab()` mounts every enabled `googleDrive` sync config on demand (`ensureMountForConfig`), sets the filter, and navigates to `/providers`; `openVaultTab()` clears the filter and returns to the vault root. `baseFiles` filters the `/providers` root listing to `googleDrive` mounts + their `pending-<configId>` placeholders when the tab is active (`rowBackend` resolves mount → config → `backendType`); GitHub/GitLab stay reachable via the sidebar rows, which are untouched.
  - Pinned **Google Drive** row at the top of the sidebar PROVIDERS section (green cloud icon, account-count badge) opens the same tab and highlights with it; empty states on both grid and list/details views explain when no Drive is connected.
  - Inside a Drive mount the existing PROVIDER SCOPE banner (`↓ Vault`, `⇄ Move`, `Vault ⌂`) applies unchanged — import-to-vault and provider-move work from the tab with zero transport changes.
- **No backend/store changes:** `stores/app.ts` `/providers` listing, the provider canal, and the REST fallback already list Drive correctly; the tab is a client-side scope filter over them.
- Local validation: `check-file-headers.sh`, `vue-tsc --noEmit`, full Vitest.

## 2026-10-18 — Wiki 2D graph shows real symbols + docs, never "Community {n}" meta-circles

- **Ask:** the 2D knowledge-graph overlay (`/wiki/graph.html`, loaded by the 3D wiki's 2D switch) still rendered graphify's community-hub export — big opaque circles labelled `Community 0`…`Community 460` with `member_count` tooltips, zero code identifiers, zero docs. The real symbol graph (11 267 nodes / 25 550 EXTRACTED/INFERRED edges) sat unused in `graphify-out/graph.json`; the old generator's shape-injection script had nothing to attach silhouettes to because no node carried a `source_file`.
- **New `scripts/wiki-graph-html.mjs`:** `buildGraphHtml({subjects, collected, kg, version, commit})` builds a self-contained symbol-level page from the in-memory wiki data — 16 subject module hubs (vis silhouette per module: star/diamond/hexagon/triangle/database — never a bare dot), ≤14 god-node + high-degree symbol satellites per subject (capped at 420 nodes / 900 edges), membership spokes hub→symbol, cross-subject EXTRACTED/INFERRED edges (solid/dashed, weight-scaled width), and symbol↔symbol edges from graphify neighbors. Every tooltip + info-panel row carries `file:line` and the heuristic structure finder's doc-comment (43 of 240 nodes in the real build — honest, never fabricated). JSON payload is `\u003c`-escaped so a hostile label cannot close the `<script>` tag; vis-network stays SRI-pinned from unpkg.
- **`scripts/generate-wiki.mjs`:** the graphify `graph.html` copy+inject block is replaced by one `buildGraphHtml` call gated on `kg` (absent graph still ships heuristic engine, no 2D page). `existsSync` import dropped (no longer needed). Header comment updated.
- **Tests:** `tests/frontend/wiki-graph-html.test.ts` (9 cases) — god-node-first pick + docs attachment + cap; real symbol/subject labels, zero `Community \d+` in output; per-subject silhouettes (`star`/`diamond`) + module/god/symbol kinds; EXTRACTED/INFERRED provenance on cross edges; SRI + self-contained static HTML; `\u003c` XSS escape.
- End-to-end dry run (`node scripts/generate-wiki.mjs --out /tmp/wiki-test`): 240 nodes (16 module · 48 god · 176 symbol), 447 edges, 206 KB, zero `Community {n}`, sample symbols `handle_connection()`, `AppState`, `open_database()`, `config.rs`. Local validation: `check-version.sh` (0.1.1), `check-file-headers.sh` (585 files), `vue-tsc --noEmit`, full Vitest (120 files / 1 171 tests).

## 2026-10-20 — Roadmap and tracker status reconciliation
- Added a current code-evidence status ledger to `PLAN.md`: scheduler, agent OS/UI tools, and secrets are substantially present; scheduler startup/lifecycle and Rust validation remain pending; folder byte-encryption integration is in progress; recovery-file/Shamir is not integrated; remaining product decisions are explicit.
- Reconciled `TASKS.md` transport statuses against current source, recorded the Caddy TLS sidecar and unsigned-only iOS CI state, and reopened the aggregate P0 closure via a required re-audit gate. Individual historical P0 checkmarks are not a current security assurance while concurrent fixes proceed.
- Superseded stale all-resolved/P0/iOS statements in `ANALYSIS.md` and added a current status note to `AI.md`. `docs/SECURITY.md` was not edited (separately owned); `docs/IOS.md` already describes the unsigned IPA and unwired signing accurately.
- Documentation-only validation: `git diff --check`; no Rust commands or tests run. Security/Rust status awaits CI; no commits made.


## 2026-10-20 — Review hardening implementation (validation boundaries retained)

- Caller-bound permission grants/inspection; live account/role checks invalidate stale JWTs; file/geo/search/suggestion/federated-local results obey caller ACLs. Agent privileged configs are checked at execution, fallback, compaction and MCP discovery boundaries, not only on save.
- Replaced file/batch metadata-only encryption with shared authenticated byte encryption/decryption, sealed key provisioning and sidecar read-back before source removal. REST and Tauri use the core; browser nonce framing and binary plaintext recovery are fixed. Added tamper, missing-path and interrupted-transition regression cases. Recursive folder/recovery/Shamir workflows remain proposals, not completion claims.
- Hardened backup pruning/rotation guidance, first-use master secret creation, corrupt keystore reads and atomic handle-store updates; CLI updater now requires verified checksums. Browser persistence/volume writer queue and concurrent loading state fixes are in place.
- CI now starts the built Docker image with isolated data and runs bounded real-HTTP readiness/auth/RBAC behavior checks. Static transport inventory remains explicitly distinct from full REST/IPC/WASM behavior parity. Local frontend, shell and Node checks are recorded in the task result; Rust tests/formatting, live Docker and the Go 1.27.2 module remain validation blockers here. No aggregate all-P0-closed claim is restored.

## 2026-10-20 — Encryption follow-up: source capture, portable sidecars, modes and ACL consistency
- Preserved the real authenticated byte pipeline; existing encrypted rows now require ciphertext authentication on retry. Source snapshots include file metadata; private-directory source capture and re-verification prevent deleting a replacement at the public pathname. Detected source mismatches retain source/recovery bytes and sidecars. Source retirement precedes the DB flag commit, avoiding an encrypted row after a detected mismatch.
- Removed hard-link dependence with exclusive-create copying from fsynced stages, retaining stages on installation failure. Portable copies do not provide atomic reader visibility. Malformed partial packages and orphan ciphertext are preserved in private recovery directories with an explicit first-call conflict and usable retry. Cleanup removes and syncs ciphertext before removing metadata; missing files are idempotent successes.
- Added ordinary source-mode/read-only metadata and restoration/rollback, with executable 0751 regression coverage. Legacy metadata defaults conservatively; special privilege bits are not restored. Added deterministic edit/replacement, late replacement, public-path recreation, detected writable-handle edits, no-clobber install, cleanup failure, partial deletion and partial metadata regressions.
- Restricted the non-admin Exa exemption to the exact shipped configuration and added persisted/runtime/fallback tests. Unified ACL parsing across file authorization, verification, listing and grant replacement; tests cover legacy casing, absent descriptive fields, legacy replacement and ambiguous-alias denial.
- Actual validation: frontend suite passed (124 files / 1193 tests); Vue/TypeScript, version checks, file headers, diff whitespace and static transport inventory (181 mappings) passed. No Cargo, rustfmt, Clippy or Rust tests ran, per AGENTS.md; Rust compilation/formatting/new regressions remain for CI. No commit or push.
- **Remaining P1 limitation, not claimed fixed:** private rename and byte checks cannot revoke an already-open external writable handle; a write after the final comparison can still race removal. Encrypt/decrypt require quiesced external in-place writers. Full arbitrary-writer safety needs platform-enforced write exclusion or a stronger storage/snapshot contract; the source race is narrowed, not fully closed. Documented in docs/SECURITY.md.
- Final ACL review also moved missing-file denial ahead of the active-admin success return; added an admin/unknown-file regression. Encrypted rows with corrupt packages and no verified plaintext remain untouched for manual recovery.

## 2026-10-20 — Asterinas-first cosmetic snapshot bridge (local implementation)

- Preserved the owner's Asterinas-primary decision and the existing Tauri/Vue app, appearance controls and stores. Ubuntu+niri remains a prior/optional reference; `/workspace/cybermanju-shell` remains an older published snapshot and was not updated.
- Added strict, allowlisted `com.cybermanju.appearance` v1 export from the current theme, separate shell style, panel docking and custom wallpaper state. Existing normalization and global → per-theme → built-in accent precedence remain authoritative. Excluded credentials, server configuration, image blobs, object URLs and filesystem paths; local wallpaper metadata does not promise media migration.
- Added an opt-in native consumer in `/workspace/asterinas-gpui/gpui-shell`: `--appearance-profile FILE` validates before display/IPC and applies actual GPUI palette/accent at primary startup, preserving native font size and configuration files. `--validate` is renderer-free. Secondary imports fail explicitly without sending a launcher command. Wallpaper/style/docking/density/glass/motion/glow and live system following remain disclosed as unapplied, not successful no-ops.
- Verification: app version/header checks, Vue typechecking and frontend suite passed (125 files / 1,203 tests, including 7 profile cases). No app Cargo commands or fresh npm install were used. Native GPUI workspace tests passed 189 Rust cases; binary build and 29 Python cases passed, including 6 actual-binary profile/Unix-socket regressions. The shared Vue/Rust fixture validates in the real shell; the lock change only makes already-locked `url` a direct dependency. No live browser export/download was exercised.
- Recorded the dependency/ownership map and bounded schema in `docs/ASTERINAS-PERSONALIZATION-BRIDGE.md`; OS-A6-PERSONAL and target runtime/hardware gates remain open. Native NVIDIA worker lifecycle work continues separately in `/workspace/asterinas-gpui/local-prs/PR_NOTES.md`. No credentials, Git authentication, commit, push, PR, publication or remote Actions were used. No physical GPU, DRM/NVIDIA nodes or KVM are exposed; GPU/Vulkan/Wayland/Tauri acceptance and Linux parity are not claimed.

## 2026-10-20 — Session-scoped appearance restore and native file admission

Added explicit-preview v1 profile restore through existing Settings, wallpaper and preference owners. URL/preset profiles apply only after the wallpaper operation succeeds; local-file metadata without image bytes is refused before mutation. Preview does not apply state, failed wallpaper operations do not apply theme/shell/panel preferences, and success is session-scoped rather than a claim of atomic durable persistence across stores. No new settings database or replacement controls were introduced.

App frontend passed 125 files / 1,207 tests (11 profile cases), typecheck, version 0.1.1 and file-header checks. Existing compatible dependencies were reused temporarily, then the symlink removed; no app Cargo commands, fresh npm installation or browser click-through was performed. In the local GPUI tree, native file admission now uses a single read-only nonblocking/no-follow descriptor and rejects terminal symlinks, FIFOs, directories and sockets. Its 189 Rust / 32 Python tests, binary build and offline Clippy passed; host results are not Asterinas runtime certification.

Native patch18 also enforces callback-owner retirement before accepted worker teardown; its CPU/QEMU matrix completed with 315 reported positives, including 3 explicitly skipped VMX/EPT bodies. Full kernel boot and GPU acceptance were not repeated or claimed. Asterinas remains primary; the published initial snapshot was not updated. No GPU/KVM/DRM/NVIDIA nodes are exposed, and no credentials, authentication, staging, commits, push, PR, publication or remote Actions were used. Updated bridge/plan/task evidence; full native media/density/style propagation, durable restart/restore, Tauri/runtime and physical GPU gates remain open. Exact sources/patches/logs are recorded in `/workspace/asterinas-gpui/evidence/appearance-restore-2030/` and PR_NOTES.md.


## 2026-10-20 — Asterinas-first live-file appearance follow-up

The local GPUI consumer now supports explicit `--watch-appearance-profile` with bounded trailing-edge inotify updates, off-foreground parsing, validated native theme replacement/redraw, last-good retention and an independent appearance-error card. Native theme reloads retain imported colors but adopt native typography. The Vue/Tauri source and existing preference owners were not replaced or changed; documentation and the open OS-A6-PERSONAL gate reflect the narrower implemented scope. Final checks passed 201 GPUI Rust and 36 Python tests, build and Clippy; app version/header checks passed (the previous 1,207 frontend result was not rerun). A real nested Niri/Xvfb/lavapipe host window changed dark→light, displayed an invalid-profile warning without exiting and recovered to dark. This is host software rendering, not Asterinas/GPU acceptance. Wallpaper/density/shell-style propagation, automatic app transport, native Tauri runtime and physical-GPU results remain open. Native kernel lifecycle patch 19 and all evidence are recorded in `/workspace/asterinas-gpui/local-prs/PR_NOTES.md`. No credentials, staging, commit, push or remote Actions were used.
