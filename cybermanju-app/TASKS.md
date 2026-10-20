# TASKS.md — CyberManju OS Full Audit → Action Plan

Source: full repo analyze 2026-10-07 (v0.1.1, `com.cybermanju.os`).
Scope: bugs, problems, philosophical improvements for the cross-device app
(Tauri desktop | Docker REST `:3456` | Pages WASM+OPFS).

**Status reconciliation (2026-10-20):** this tracker contains historical
checkboxes and audit notes. The aggregate claim that all 24 P0s are closed is
**reopened**: concurrent security fixes are in progress and the Rust side has
not been re-verified in CI for this pass. Individual `[x]` marks record prior
implementation claims, not a present security assurance. See the P0 re-audit
gate below. Transport items updated here are checked against current source;
this is not a Rust validation run.

Legend: `[P0]` critical/security-data-loss · `[P1]` cross-device parity · `[P2]` robustness/ops · `[P3]` philosophy/debt.
Checkboxes are intentional — tick as you land.

---

## P0 — Auth / AuthZ (prior findings; aggregate closure is not verified)

- [~] `P0-REVIEW` Re-audit all 24 historical P0 closures after the concurrent
  security changes; inspect code/tests and require green CI before restoring an
  aggregate closed status. Do not treat this review gate as a claim that all
  24 underlying fixes are known broken or known secure.

- [x] `P0-1` Enforce object-level file access on REST — landed: `api::files::check_access` + `deny_file` gate at every content/share/bath/agent/os-exec arm (`lib.rs` incl. `GET/PUT/DELETE /api/files/:id/content`, share-grant, batch, agent jobs, `os/exec` owner); token-capability share bytes stay public by design. (2026-10-11 audit.)
- [x] `P0-2` Viewer read-only — landed: `authorize` denies all write methods for `role == "viewer"` (`security.rs`), pinned by viewer tests. (2026-10-11 audit.)
- [x] `P0-3` Authenticate Tauri IPC — `register/create/update/delete/list_users`, `set/grant/revoke_permission` take only `State<AppState>`. `LocalIpc` mode allows arbitrary `admin` creation. Add local auth / capability gate. Landed 2026-10-11: IPC capability gate (`ipc_caller`/`require_ipc_admin` in `src-tauri/.../users.rs`, bootstrap-open, admin writes need an admin session; frontend passes `sessionToken`).
- [x] `P0-4` Strip `password_hash` on Tauri — landed 2026-10-11: `list_users` projects rows through JSON and removes `passwordHash`/`password_hash`, mirroring web `list_users_safe` (frontend `User.passwordHash?` already optional — no UI change).
- [x] `P0-5` Unify token systems — web persistent JWT vs Tauri `OsRng`-fresh `hmac_secret` per boot (`lib.rs:66-69`). Desktop sessions die on restart, JWT lives. Shared revocation needed; move revocation/backoff/rate out of in-memory `AuthState` (restart wipes `revoke()`, multi-replica shares nothing). Landed 2026-10-11: revocations persist (`revoked:*` kv rows, hydrated at boot, saved on logout) + stable Tauri HMAC secret (load-or-create 0600; backoff/rate stay per-process by design).
- [x] `P0-6` Fix `restore_config_token` — landed 2026-10-11: shim deleted (`lib.rs`), `POST /api/sync/configs` relies on `save_config`'s side-table write only; pinned by `spine.rs` no-token-in-response test.
- [x] `P0-7` Close SSE auth gap — `lib.rs:433-441` intercepts SSE pre-`handle_request`; EventSource can't send `Authorization`. Enforce query-token in `handle_sse_connection` or remove live tail. Landed 2026-10-11: SSE accepts `?token=` (header first, query second, same JWT gate; missing credential answers 401, never a stream).

## P0 — Agent RCE / permission bypass

- [x] `P0-8` Gate `save_config` MCP servers — landed: `config_needs_admin` + `save_config_as(db, config, is_admin)` (`agent_api.rs`), REST passes claims; stdio/non-default MCP, `auto_approve` and blanket `bash` allow answer `auth:` for non-admins. (2026-10-11 audit.)
- [x] `P0-9` Re-auth `remember:true` — `approve_job:2063-2085 remember_allow(tool)` persists `Allow` with no re-auth, silent fail. One approved `ask` becomes permanent. Require explicit confirm + audit log. Landed 2026-10-11: remember-allow allow/skip/fail all hit the audit log (who/when/which tool).
- [x] `P0-10` Scope jobs/approvals per-user — landed: `AgentJob.owner_id` + `check_job_owner` on every status/abort/approve entry, `claims_owner` scoping on REST, owner-bound transcripts; new todos API follows the same posture. (2026-10-11 audit.)
- [x] `P0-11` Fix containment TOCTOU — `normalize_join/join_contained:840-874` explicitly no symlink resolution, `..` popped lexically, `read_raw→apply_edit→tool_write` non-atomic (`agent_api.rs:1337-1347`). Add symlink resolution + file lock / atomic write. Landed 2026-10-11: symlink resolution + containment re-check in `join_contained`, atomic temp+rename writes in `tool_write`.
- [x] `P0-12` Scope `POST /api/agent/memories{,/recall}` per-`config_id` — `recall` without `configId` scans all configs keyword-only. Landed 2026-10-11: `AgentMemory.owner_id` rows, legacy-readable, route/tool/compaction stamping + scoped list/recall/delete.

## P0 — Crypto / data-loss

- [x] `P0-13` Fix hybrid encrypt needing private key — `encrypt_data:420-455` derives X25519 static from ML-KEM SK. Public-key-only encryption impossible; ML-KEM compromise kills both halves. Independent classical keypair. Landed 2026-10-11: independent stored X25519 pair (public-key-only Hybrid encrypt; sealed persistence; legacy derivation fallback).
- [x] `P0-14` Key separation — `encrypt_with_symmetric_key:486-523` uses `private_key[..32]` raw (no KDF) for `MlDsa44/65/87, ClassicalSign, Aes256`. ML-DSA seed doubles as signing seed + enc key. Per-purpose KDF. Landed 2026-10-11: HKDF per-purpose data keys (algorithm-bound info); decrypt tries KDF first, legacy raw slice second.
- [x] `P0-15` Fix algorithm labels — `algorithm_from_str:762-775`: `kyber768|kyber512→Hybrid`, `sphincs+→ClassicalSign` (HMAC, not PQ), `Aes256` is ChaCha20Poly1305. UI `nist_level Hybrid=5` overstates (768=L3). Landed 2026-10-11: Hybrid nist_level 5→3 + honest display names; legacy aliases documented, still parseable.
- [x] `P0-16` Bulk-restore CPU DoS — `seal/open_sealed` fresh Argon2id per blob; `restore:815-821` derives per-file. Cache derived key / streaming restore. Landed 2026-10-11: bounded KEK cache (8 entries, passphrase never stored) in `open_sealed`.
- [x] `P0-17` `data_dir()` fallback to `temp_dir()/cybermanju` when `HOME` unset — predictable world-readable `/tmp`. Fail-closed instead. `get_or_derive:190-220` silently re-wraps on passphrase change — warn loudly, keep old key. Landed 2026-10-11: `data_dir` fail-closed (no `/tmp` fallback) + re-wrap refused with `auth:` (old key kept).
- [x] `P0-18` Hash-anchor minimum — `agent/edit.rs:21-30 actual.starts_with(anchor)` with no minimum; 4-hex-char (16-bit) passes despite “64+ bits” comment. Enforce `anchor.len()>=16`. Fix `strip_anchor` dropping legitimate trailing `[blake3:64hex]` lines.

## P0 — Sync / lease / repair correctness

- [x] `P0-19` Lease CAS — landed 2026-10-11: acquire/renew/release read-decide-write inside ONE write txn; concurrent-racer regression test elects exactly one winner. Cross-process mutex stays out of scope (single-file redb, one opener); cross-device story is devices + op-log. `default_holder()` still prefers `CYBERMANJU_DEVICE`/hostname.
- [x] `P0-20` `decide_write` Overwrite loses history — `332-351` comment says “vectors merged” but code just `Apply`. `suggested_id` uses `process::id()` — cross-device collision. Real merge or rename. Landed 2026-10-11: Overwrite returns `ApplyMerged` (vectors max-merged, history kept) + deterministic KeepBoth ids (no PIDs).
- [x] `P0-21` Durable job poll — `MAX_LIVE_RUNS=16 (state.rs:198)`, `SYNC_RUN_HISTORY_LIMIT=20 (db/database.rs:95)`. Old `jobId` → `not_found` after eviction/restart. Persist runs, or return `expired:` not `not_found:`. Landed 2026-10-11: evicted ids answer `expired:` (bounded 64-id memory + history-table fallthrough).
- [x] `P0-22` Write-lock serialization — every `POST/PUT/DELETE` holds process `RwLockWriteGuard (lib.rs:1174-1179)` + `disk/repair/os` handlers run under `db.write()`. Long download/upload stalls all writes + starves readers. Spawn long work (per existing `Long work must be spawned` comment). Landed 2026-10-12 (closed, not just disciplined): bulk-transfer + detached-job paths run pre-lock with scoped locks; os_exec/disk stay guarded for logical atomicity (documented at the lock site); request bodies capped at 100 MB (`MAX_BODY_SIZE`, Caddy-aligned); single-opener `OpenerLock` (`crates/db`: `<db>.lock` create_new guard, second opener fails closed with `conflict:`, stale-PID reap, release-on-drop, regression test); frontend pins 15 s abort + backoff retry in `restFetch` (`tests/frontend/rest-hardening.test.ts`).
- [x] `P0-23` Unbounded growth — `PENDING:226` flush only “at top of any request holding write lock” (idle server holds repair/scrub in RAM); `FINDINGS:86` uncapped; `GET /api/repair/status:1296-1319` clones all rows. Cap + paginate. Landed 2026-10-11: FINDINGS capped (2000, regenerable), PENDING backpressure (10k + surfaced refusals), paged repair status (200 default / 2000 max + `repairs_capped`).
- [x] `P0-24` Rebuild flattens hierarchy — `write_rebuild_records:1234-1276` creates `FileNode{parent_id:None}` — all recovered at root. Preserve parent paths. Landed 2026-10-11: rebuild recreates parent folders from `original_path` (deterministic `rebuild-dir-*` ids, idempotent).

## P1 — Frontend transport parity (`src/composables/useTauri.ts`)

- [x] `P1-1` `isTauri():99-104` checks the truthiness of `window.__TAURI__` so the static build's `false` sentinel is not misidentified as native. Settings uses the shared helper and StatusBar derives from `useTransport()`; regression tests cover false sentinel and truthy bridge.
- [x] `P1-2` Shared `useTransport()` — replace binary `isWebMode()/isStaticHost()` labels with one 3-state helper `(label/short/tone/backend: tauri|rest|wasm)`. Fix `StatusBar.vue:80`, `KeyboardShortcutsHelp`, `ShellShortcutDock` (currently `WEB/TAURI`); `CodeStudio.vue:380` 3-way is the pattern to keep. Browser-only Alt+ fallback guidance is now explicitly scoped to browser mode, not just WASM.
- [ ] `P1-3` Static-host health probe — still open. `probeStaticConnection()` is a sync-provider credential/connectivity test, not an app-dashboard readiness probe; `isStaticHost()` still classifies based on runtime/server URL rather than probing `/api/readyz` with fallback.
- [x] `P1-4` WASM first-paint flash — transport labels in Settings, ShellShortcutDock, and LandingPage use the synchronous shared `useTransport()`/`isStaticHost()` classification; `wasmBackendActive()` is reserved for backend readiness after loading. Regression test confirms Pages reports WASM before the async module loads.
- [x] `P1-5` Sync split-brain — the sync domain is `REST_FIRST` (single REST job/poll source) with `core.invoke` fallback when the dashboard is down; comment and routing are in `src/composables/useTauri.ts`.
- [x] `P1-6` `os_write` parity — `PUT /api/os/write` is registered in `REST_ROUTES` + `REST_FIRST`, the Rust handler shares the volume writer, and `transport-routes.test.ts` guards method, path, body, and transport selection.
- [x] `P1-7` OS path encoding — `os_stat/ls/du` send paths through `?path=` with `encodeURIComponent`; regression tests cover spaces, slash preservation, and parent segments as encoded query data.
- [x] `P1-8` Paginated WASM search — `invoke()` explicitly uses the local BM25-lite search and returns `{results,total}` with offset/limit slicing rather than throwing or falling through to unpaginated results.
- [~] `P1-9` Transport naming/crypto coverage — WASM has static handlers for crypto commands, but `WRITE_ONLY_COMMANDS` remains the name and pure REST still lacks equivalent crypto endpoints. Documented gap remains; do not count this item complete until naming/coverage is resolved or explicitly accepted.
- [x] `P1-10` REST failure handling — `restFetch()` normalizes network/HTTP prefixes, preserves plain-text errors, retries transient failures, and enforces its 15s deadline through both headers and body consumption; `probeStaticConnection()` returns `unsupported:` for unknown browser probes. `rest-fetch-timeout.test.ts` covers a stalled body and successful retry.
- [x] `P1-11` `pathExists:2167-2175` bypassed WASM and treated any successful `/api/files` response as an existing path. It now checks the requested path from `invoke('list_files')`, which follows REST or the local WASM DB worker.
- [x] `P1-12` WASM parity-gap visibility — `notifyError` increments `wasmGapCount` and logs numbered `[WASM parity gap #…]` details; the silent `[WASM Mode]` suppression described in the original finding is no longer present.
- [ ] `P1-13` Store loading/errors — single `lastError` string + shared `isLoading` races (`fetchFiles:327-379` parallel in `initialize:275-284`). Per-domain `loading{}` / error queue. `initialize:270-289` omits `fetchOsPs/dashboard/disks/agent`; `startAutoRefresh` omits `disks/os/agent`; gate `StatusBar:128-133` 4s `fetchOsPs` poll on `!isStaticHost()||document.visible`.
- [x] `P1-14` Agent job transport — resolve SSE against the configured server or current REST dashboard origin; Tauri-IPC-only and static WASM without a server bypass the dead localhost `/events` probe and poll `invoke('agent_job_status')`. Pure resolver tests cover native/static fallback and explicit/same-origin REST URLs.
- [x] `P1-15` Settings display — `SettingsPage.vue` documents `VITE_TRANSPORT`, uses the shared transport indicator, and preserves the server URL's original case for copy/paste.

## P1 — Offline / OPFS / volume

- [ ] `P1-16` Single volume writer — triple writers to `cybermanju.os.volume`: `useTauri.ts:1391-1418` whole-blob JSON, `crates/os-wasm/src/os.rs:34,95-144`, `useVolumeMirror.ts:21`. Read-modify-write race + 5MiB quota. Rust `save_volume` silently drops return. Add locking/dirty-merge, surface quota (`disk_full:` already correct on TS side).
- [x] `P1-17` Cross-language 1MiB cap — Rust and TypeScript keep separate constants; a frontend contract test parses Rust `MAX_WRITE_BYTES` and asserts it equals `STATIC_WRITE_LIMIT`, preventing WASM shell writes from drifting across transports.
- [x] `P1-18` Pipe refusal UX — static cybsh now dispatches `&&`/`;` clause-by-clause across vault-aware handlers and the WASM volume fallback, preserving fail-fast output. Unquoted `|` gets an explicit `unsupported:` result; quoted bars remain ordinary arguments. Interceptor exceptions are logged before fallback. Regression tests cover `quota && ls`, short-circuit failure, and pipe quoting.
- [ ] `P1-19` WASM load resilience — [x] a rejected `loadWasm()` import/init promise is evicted so the next interaction can retry; regression test covers transient failure and successful retry. Remaining: `wasmDbBackend:308-317` caches `opfs|memory` forever (no upgrade if OPFS appears), and `dbWorker:502-542` has 30s/120s timeouts with no `navigator.onLine` fast-fail.
- [ ] `P1-20` Vault boot — `useVault.ts:64-93 kvKnownBroken` flips permanently on one `unsupported:` (correct for old pkg) but `vaultBackend()` stays `unknown` until first call. Call `wasmDbBackend()` at boot so `AccountManagerPanel.vue:905` warning is accurate. `migrateVaultFromLocalStorage:146-165` good — keep.
- [x] `P1-21` Host filesystem safety/parity — host browsing uses `useTransport()` so REST dashboards are not misrouted to browser storage; reject filesystem/mount roots as operation sources, `..` escapes, and self/descendant copy/move targets; verify copied WASM bytes before deleting a move source. Host text writes enforce the 1 MiB cap on UTF-8 bytes and preserve quoted/empty content; regressions cover REST routing, roots, traversal, recursion, corruption, quoting, and multibyte writes.
- [x] `P1-22` Backup restore safety — TypeScript/Rust cap images at 256 MiB and KDF work at 1M rounds, validate container metadata and declared LZ4 output before expansion, preflight legacy redb images in isolated memory before replacing OPFS, check native snapshot size before materializing bytes, and require explicit confirmation before browser vault replacement.

## P2 — Tests (28 files / ~336 tests, not 25/281)

- [ ] `P2-1` Fix AGENTS.md counts + add contract tests: `POST /api/sync/start→202{jobId}` + poll `/jobs/{id}`/progress/cancel; `POST /api/agent/prompt→202` + approve/abort, 10-min auto-deny, `deny`-beats-auto-approve.
- [ ] `P2-2` Lease/repair/GC tests: `POST /api/lease/acquire|release`, `GET /api/lease/status`, TTL + steal-after-expiry, live-lease → `conflict:`; `POST /api/scrub/run`, `GET /api/scrub/runs`, `POST /api/repair/run|rebuild|gc`, refcount-GC `dryRun`, `PUT|GET /api/volume/block/{lba}`, RS `k+m` placement, two-device concurrent-write / offline-reconnect / admit-before-upload `disk_full:`.
- [ ] `P2-3` Transport parity test: boot real `crates/web` (claimed 135+ routes in `lib.rs`), assert every `REST_ROUTES.buildPath()` served + `tauri/rest/wasm` shape equality (`snake→camel`, `_key` stripping) + full AGENT-1 prefix→hint matrix (`auth:/rate_limited:/not_found:/unsupported:→501/too_large:/integrity:/network:/disk_full:/conflict:` + `invalid:/context:/limit:`).
- [ ] `P2-4` WASM twin tests: compression (browser `lz4/zstd/brotli/triple` via `@dweb-browser/zstd-wasm` vs desktop `zstd-sys` — assert identical frames both ways), `X25519 (ML-KEM slot)` label, `tree-sitter` vs `heuristic engine` flag, FSA-Chromium vs Firefox/Safari Export/Import fallback — currently documented honest limits with no regression test (`container.test.ts` injects Node primitives to avoid wasm bundle).

## P2 — CI / versions / deploy

- [x] `P2-5` Gate audits — resolved 2026-10-18: **gate**. `cargo audit` + `npm audit` lost `continue-on-error` in `ci.yml`, GitLab lost `allow_failure`/`|| true`. Unblocked by the quinn-proto 0.11.19 / rustls 0.23.46 / ruzstd 0.9.1 lock bump (10 RustSec vulns fixed; npm production clean). Added `cargo metadata --locked` lock gate to `rust-check` so lock edits stay resolution-exact.
- [ ] `P2-6` Docker smoke — `ci:docker-build` only `build push:false + docker images`. Add `compose up` + `GET /api/readyz` + one `POST /api/...` smoke (same GitLab). Assert `read_only/cap_drop/no-new-privs/mem_limit` hardening.
- [x] `P2-7` Fix flaky artifact chain — resolved 2026-10-18: `flatpak-build` `Download Debian package` lost `continue-on-error` in **both** `ci.yml` and `release.yml` — the deb is load-bearing (manifest `type: dir → ../../artifacts`), so it now fails fast at download instead of a misleading late `flatpak-builder`/`if-no-files-found` failure; no rpm `download-artifact` chain remains (uploads stay `if-no-files-found:error` by design); `arch-build` `needs: nothing` is documented inline (queue latency, full rebuild inside an Arch container).
- [x] `P2-8` Android signing parity — `release.yml REQUIRE_SIGNING=1` hard-fails; `ci.yml` PR/fork builds publish **`*-unsigned.apk` explicitly named `unsigned`** (`android-signing.sh verify` renames; never neutral). Emulator smoke via `scripts/android-smoke.sh` (best-effort, skips without arm64 image); release ships **APK+AAB `--split-per-abi`** (arm64-v8a only, documented `ort-sys` reason); `versionCode` pinned (`major*1e6+minor*1e3+patch`, checked); update channel documented (`docs/ANDROID.md` §7).
- [ ] `P2-9` Pages preview on PRs — `deploy-pages needs:wasm-build, push-main-only`. PRs never verify `dist-wasm/index.html`. Add preview artifact. GitLab `pages:` only `cp dist-wasm → public/` + `test -f index.html`, artifact expires 1 week, no publish config.
- [ ] `P2-10` Release parity — GitHub `create-release` hard-fails if any of `windows/linux/rpm/flatpak/arch/macos/android/wasm` missing; GitLab `windows/macOS manual+allow_failure, OPTIONAL=1` routinely ships without exe/msi/dmg. Align policy.
- [x] `P2-11` Version script hardening — green today (`0.1.1` everywhere) but `check-version.sh` + AGENTS.md comments still say `0.1.0`; matchers break on `version.workspace`/indent/README reformat; misses Android `versionCode` (gitignored `src-tauri/gen/`), iOS (no target), Flatpak version, Docker `:latest`, `Cargo.lock`; tag only checked with `--tag` (release time, not `main`). Fix comments, loosen matchers, add carriers or explicit exclusions.
- [x] `P2-12` Compose/docs unification — `docker-compose.yml` hardened vs `ARCHITECTURE §9` stale insecure (`3456:3456`, no hardening, `healthcheck /api/health` vs `OPERATIONS:12` + compose `/api/readyz`). Copy-paste opens WAN + wrong probe. Single source + CI `diff` check. Pin `x-casaos.icon` URL or CI-check it. Assert `vite.config.wasm.ts base:/` + `port_map/index:/` consistency.
- [ ] `P2-13` Pages identity — `ARCHITECTURE §11` (read-only demo / remote REST) vs `OPERATIONS §4c+§8` (local vault: localStorage+BM25-lite, server ops → `unsupported:`). Pick one. Document Pages→NAS CORS breakage (`https://<user>.github.io` → `http://nas:3456` gets no `ACAO`): proxy/CORS allowlist procedure.
- [ ] `P2-14` Dockerfile vs CI wasm parity — `Dockerfile` Stage-1 `DOCKER_BUILD=true build:wasm:frontend` (stub plugin, no `wasm-pack`) vs CI real `wasm-pack build crates/os-wasm`. Docker-static vs Pages differ in exactly the documented caps (`zstd/triple`, ML-KEM label, compression endpoint). Parity test or single build path.

## P2 — TLS / mobile

- [x] `P2-15` TLS proxy — `docker-compose.tls.yml` adds Caddy in front of the cleartext app service; `Caddyfile` provisions site TLS, emits HSTS/security headers, and caps request bodies at 100 MB. The app intentionally remains no-TLS; this closes proxy delivery, not direct `:3456` access. Review `docs/SECURITY.md` (owned separately) for current forwarded-header/trust and OAuth caveats; do not infer direct-LAN protection.
- [~] `P2-16` Mobile story — Android native delivery is documented in `docs/ANDROID.md`. iOS has a macOS CI job producing an unsigned arm64 device IPA (`.github/workflows/ci.yml`, `release.yml`, `docs/IOS.md`); `scripts/ios-signing.sh` exists but is not wired into workflows. Signed release/TestFlight with real Apple credentials remains outstanding. Do not describe iOS as zero or as a signed build.
- [x] `P2-17` Doc drift — `generate_handler!` ~100 handlers not “35”; `commands/mod.rs` 19 modules not 20; `Cargo.toml` 16 members not 15 (`+docker/server`); `ROUTED_SEGMENTS security.rs:119-156` coarse L2 only. Fix AGENTS.md or CI-assert counts.

## P3 — Philosophical (what “better/different” looks like)

- [ ] `P3-1` **One Backend interface, not three apps.** Define `Backend` (files/search/os/disk/sync/agent/db/kv) with capability flags (`supportsStreaming`, `maxWriteBytes`, `engine`); `unsupported:` as typed result, not exception string. Gate by capability + permission, not `isTauri/isStaticHost`.
- [ ] `P3-2` **Local-first log, not server-first files.** `redb` single-file (exclusive lock forbids double-open Docker+desktop on same `/data`) + file-bytes sync (not DB deltas) + local-only leases = split-brain. Ship op-log / CRDT deltas (sync the `vv` vectors you already model), content-addressed blocks (BLAKE3 + RS `k+m` already there), CAS leases.
- [ ] `P3-3` **Offline as a state.** OPFS R-M-W + quota + `[WASM Mode]` suppression + localhost-hardcoded SSE make offline an accident. Explicit `offline/readonly/degraded` UI states with queue-and-replay, dirty-merge, quota surfacing.
- [ ] `P3-4` **AuthZ before features.** Object ACL + per-config agent ACL + per-user jobs + viewer-read-only + persistent revocation. Fail-closed means nothing without it.
- [ ] `P3-5` **Test the contract.** One parity test booting real `crates/web` asserting every mapped path + shape equality + AGENT-1 matrix beats 336 unit tests. Add sync/agent lifecycle, lease TTL/steal, GC dryRun, RS placement, offline-reconnect.

---

### Current follow-up priorities (2026-10-20)
1. `P0-REVIEW` — re-audit historical P0 closures against current code and green CI after concurrent security fixes settle; do not report aggregate closure meanwhile.
2. File/batch byte-encryption is implemented; run the added Rust round-trip/recovery tests in CI. Recursive folder jobs and restore/trash semantics remain separate, unresolved work in `PLAN.md`.
3. Revalidate the remaining `P1-3`, `P1-9`, `P1-13`, `P1-16`, `P1-19`, `P1-20` gaps against the new dashboard identity probe, REST crypto routes and shared volume-write FIFO; store-refresh/error handling, WASM backend retry and vault boot still need independent coverage.
4. `P2-16` — signed iOS release/TestFlight remains dependent on Apple credentials and CI wiring; unsigned device IPA is already built by CI.
5. Recovery file/Shamir — decide and integrate only after product policy is confirmed; current `src/utils/shamir.ts` alone does not complete recovery.

---

## Asterinas OS integration — separate prioritized track (2026-10-20)

**Primary kernel/base: Asterinas.** Ubuntu+niri is an optional reference, not
the target base or an automatic fallback. This track adds future OS integration
work; all earlier app/security checkboxes and follow-up priorities remain
unchanged. `PLAN.md` still owns app improvements. The active architecture and
full acceptance criteria are in [the Asterinas-first plan](docs/CYBERMANJU-OS-PLAN.md#active-plan--asterinas-first-2026-10-20).
All `OS-A*` items are open/unvalidated, not claims of missing upstream features.
P0 here denotes release-blocking safety/prerequisites, not discovered app bugs.

### P0 — establish a bootable, safe and testable foundation

- [ ] `OS-A0` **Baseline and review (phase A0).** Collect the concurrent
  Asterinas/GPUI second review; pin source/toolchain/rootfs/firmware revisions,
  target models and VM configuration. Produce an ABI/ioctl/protocol and runtime
  dependency matrix with explicit implementation vs validation status. Initial
  boot, simpledrm and PCI/DMA/IOMMU/IRQ experiments are not accelerated graphics.
  Inventory drafted: [Asterinas ABI baseline](docs/ASTERINAS-ABI-BASELINE.md)
  records completed local reviews and remaining blockers; A0 stays open pending
  review, exact runtime/target pins and ownership of proposed experiments.
- [ ] `OS-A1` **Boot/userspace (after A0).** Validate boot/init/rootfs and Rust
  userspace loader/libc, threads/futex, mmap, polling/sockets, storage durability,
  permissions, entropy and networking. Gate: repeatable VM cold boot to shell,
  representative Rust service I/O, restart and shutdown logs. No assumption of
  systemd/apt or full Linux ABI from the kernel's language or compatibility goal.
- [ ] `OS-A2` **Display/seat/input (after A1).** Validate DRM/KMS/GEM/mmap,
  scanout/events, seat/session/input access, keyboard/pointer and teardown.
  Specify required DMA-BUF formats/modifiers and synchronization contracts.
  Gate: known-buffer output, mode/input/lifetime tests; CPU dumb buffers remain
  explicitly diagnostic, not native acceleration or frame parity.
- [ ] `OS-A3-AMD` **Native AMD + Vulkan (after A1/A2 interface baseline).**
  Resolve real device/ioctl contracts, memory management, submission, DMA/IOMMU,
  IRQs/reset, firmware rights/loading and userspace ICD. Gate per named physical
  GPU: real GPU execution, correct output, synchronized render/present and fault
  recovery on Asterinas; Linux amdgpu/Mesa installation is not a port.
- [ ] `OS-A3-NVIDIA` **Native NVIDIA + Vulkan (same dependencies; independent
  of AMD).** Resolve RM/GSP where applicable, firmware and vendor contracts plus
  memory/submission/isolation/IRQ/reset and ICD/presentation requirements.
  Gate: the same per-device native evidence as AMD. Neither PCI enumeration,
  simpledrm nor an AMD pass closes this item; complete RM/GSP is not established.

### P1 — integrate the desktop and preserve the product

- [ ] `OS-A4` **Compositor/presentation.** A2 permits software experiments;
  accelerated closure requires A3 for that GPU. Validate niri/Smithay or a
  documented alternative, Wayland/seat/input, required DRM/GBM/EGL, DMA-BUF/fences,
  layer-shell, scaling/hotplug and IPC. Gate: two local clients, focus/workspaces,
  reliable synchronized presentation, resize/mixed-DPI/multi-output/restart tests
  with renderer/device recorded; no XWayland shortcut counted as native success.
- [ ] `OS-A5` **GPUI shell (after accelerated A4 and that GPU's A3).** Inspect
  the actual pinned rendering backend; integrate bar/launcher/control center,
  notifications/fonts and supported desktop-service APIs. Gate: launch/focus,
  input/scaling and restart tests on Asterinas. Vault/agent/sync widgets remain
  proposals until separately implemented and tested; no premature frame parity.
- [ ] `OS-A6` **CyberManju app/API/UX.** Inventory can start at A0; native UI
  needs A1/A4 and shell integration needs A5. Prove Tauri/Wry/GTK/WebKitGTK,
  IPC, fonts, sandbox/process model and desktop-service dependencies. Gate:
  native app launch and representative files/vault/agent/search/sync/scheduler/
  secrets flows with existing AuthZ, async jobs, error and persistence contracts.
  Browser/REST demos do not prove native compatibility. Keep `P0-REVIEW`, folder
  encryption/trash/restore, recovery decisions, transport/UI and cross-device
  tests as separate app release blockers; do not duplicate or close them here.
- [ ] `OS-A6-PERSONAL` **Personalization compatibility (inventory at A0; close
  with A5/A6).** Reuse `SettingsPage.vue` controls and `src/ui/tokens.ts`: themes,
  shell style, URL/file wallpaper, accent, density, chrome and panel position.
  Preserve motion, keyboard bindings/KPL, touch gestures/JSON, locale and local
  view/sidebar/refresh preferences too; inventory current storage and distinguish
  durable settings from unverified Pinia-only persistence. Retain per-install
  wallpaper blobs and keep credentials out of cosmetic exports.
  Define a versioned adapter with app-vs-desktop ownership, not duplicate stores
  or controls. Gate: existing profile import, every supported setting propagated
  to Vue/GPUI, restart/export/restore/migration without loss, invalid/missing
  wallpaper behavior, accessibility and multi-output scaling. Report unsupported
  OS capabilities honestly; never silently reset preferences.
  Local prerequisite implemented: bounded v1 cosmetic export from the existing
  Settings state and explicit GPUI palette/accent projection with opt-in live-file
  watching, last-good error retention and native-theme reload precedence. App's
  preceding frontend run passed 1,207 tests, including explicit-preview URL/preset
  restore through existing owners; native shell passes 201 Rust and 36 Python
  tests, including 13 real-binary profile/IPC/filesystem checks. Restore is session-
  scoped; missing local wallpaper bytes are refused, not fabricated. No atomic
  durable migration, automatic app-to-shell synchronization, full settings/media
  propagation, native Asterinas app/session or physical-GPU acceptance is inferred;
  see [the bridge contract](docs/ASTERINAS-PERSONALIZATION-BRIDGE.md).

### P0 release safety / P2 qualification — design early, close after integration

- [ ] `OS-A7` **[P0] Installer/update/recovery.** Design after A1; release
  closure needs A6 and supported hardware evidence. Validate storage/encryption
  and boot mechanisms independently of app vault crypto; specify signed images/
  manifests, atomic update, rollback, offline rescue and migration policy.
  Gate on disposable VM disks: install/reboot, wrong-disk refusal, interruption,
  tampered signature rejection, failed-boot rollback, backup/key recovery. No
  inferred LUKS2+ML-KEM or Shamir completion; physical destructive tests require
  separate approval and current app recovery policy decisions remain open.
- [ ] `OS-A8-CI` **[P2; release gate] Continuous CI evidence.** Design at A0,
  require evidence per phase, qualify A3–A7 at release. Plan pinned VM boot/ABI/
  graphics/input/API and installer/recovery tests plus existing app/security
  checks; retain logs/artifact hashes and explicit unsupported results. Future
  approved CI must pass before closing gates. This docs-only pass neither edits
  workflows nor runs scripts, Cargo/npm, installs, builds or remote Actions.
- [ ] `OS-A8-HW` **[P2; release gate] Physical GPU/performance qualification
  (after A3–A6, release after A7).** Current environment has no physical GPU;
  arrange explicit AMD and NVIDIA targets and publish per-model support.
  Compare pinned Asterinas vs Ubuntu+niri on the same machine, firmware,
  resolution/refresh, renderer, workload and power/thermal settings. Gate:
  repeated raw traces for cold/warm boot, CPU/RAM, input-to-present latency,
  p50/p95/p99 frame times/missed frames, GPU utilization/power and app timings,
  variance and predeclared parity thresholds. VM/software results never close
  physical GPU or frame-parity claims; unsupported models remain excluded.

**Execution boundary:** planning edits only, kept local. No code changes,
project scripts, dependency installation, local Rust/Cargo/npm, commit, push,
PR or remote Actions in this pass. Dependencies replace the historical fixed
week estimates; neither choosing Asterinas nor writing this track completes it.
