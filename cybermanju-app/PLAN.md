# PLAN.md — CyberManju OS improvement pass: Scheduler · Agent exposure · Secrets keystore · Folder/disk encryption · Recovery

Five sequential phases, grounded in a full research pass (subagent reports with
file:line refs verified against the tree, 2026-10-09). This is the execution
plan; `ANALYSIS.md` is the ideation layer, `TASKS.md` stays the bug tracker.

## Current status reconciliation (2026-10-20)

This is a code-tree inspection, **not a CI/security sign-off**; Rust has not
been tested locally (per `AGENTS.md`) and concurrent security work is in
progress. The original phase specifications below are retained as design
history; do not treat every listed step as outstanding or every earlier
checkbox/decision as approved.

| Area | Current evidence/status |
|---|---|
| Scheduler | Substantial implementation exists: `crates/os/src/{schedule,scheduler}.rs`, `crates/web/src/api/cron_api.rs`, Tauri cron commands, REST/WASM mappings, Process-panel schedule UI and shell support. Startup wiring is implemented in `crates/web/src/lib.rs`; do not call the full lifecycle verified until CI and Docker startup are checked. |
| Agent OS/UI tools | `os_exec`, `ui_open_panel`, `ui_notify` are present in the Rust agent path, browser path, prompts and UI metadata (`crates/web/src/api/agent_api.rs`, `src/composables/useAgent.ts`, `src/utils/uiRequests.ts`). This records implementation presence, not Rust test verification. |
| Secrets | Metadata-only list, sealed values/reveal API, Tauri/REST/browser routes, panel and `secret_list`/`secret_get` agent tools exist (`crates/web/src/api/secrets.rs`, `src/components/SecretsPanel.vue`, `src/types/secrets*` / `crates/types/src/secrets.rs`). Do not infer an audit/security sign-off from this inventory. |
| File/batch encryption vs folders | Shared real-byte encrypt/decrypt is implemented in `crates/web/src/api/batch.rs` and wired to REST/Tauri; browser cipher framing, binary round-trip and failure handling are corrected. Rust regression tests are added but **not run locally**; CI is required. Recursive folder jobs, folder manifests and trash/restore integration remain unimplemented proposals. File encryption removes the original path after verified ciphertext installation; it does not silently place plaintext in trash. |
| Recovery / Shamir | `src/utils/shamir.ts` exists as a utility, but no integrated recovery-file create/unseal flow, Rust Shamir/recovery service, or recovery UI/routes were found. The recovery-file + 2-of-3 design remains a proposal, not a shipped recovery feature. |
| Transport fixes | Several old `TASKS.md` items are already implemented: static connection probing (`probeStaticConnection`), sync-domain `REST_FIRST` with fallback, `notifyError` parity-gap logging/counter, and case-preserving Settings URL display. Remaining transport gaps stay tracked separately in `TASKS.md`. |
| TLS / iOS | Caddy TLS sidecar exists (`docker-compose.tls.yml`, `Caddyfile`); iOS CI currently emits an **unsigned arm64 device IPA**. `scripts/ios-signing.sh` is not wired into CI; signed/TestFlight delivery remains open. `docs/IOS.md` is the operational source. |

**Still-open product decisions:** confirm whether folder encryption must retain
plaintext in trash until trash is emptied (and byte-level restore semantics),
the recovery artifact/passphrase and Shamir-share UX/threshold policy, and
whether recovery rotates/re-wraps existing keys. The proposed choices below
are not product approval.

## Previously proposed design choices (not product sign-off)

| Decision | Choice |
|---|---|
| Recovery | **Recovery file (`recovery.cybrec`) + Shamir 2-of-3 paper shares** |
| Folder encryption | **Purge plaintext with trash safety** (restorable until trash emptied) |
| Scheduler scope | **`.cybsh` scripts only** (agent/sync automation via script bodies calling `ai`/`sync`) |
| Delivery | **All 5 phases, sequential** |

## Repo rules governing every phase (AGENTS.md / AI.md)

- **No local cargo.** Rust written blind against docs.rs, hardened for
  `fmt --check` + `clippy -D warnings`: ≤100 cols, chains ≤60 cols or one
  method per line, no `map_or(false,…)`, no trailing whitespace, one EOF
  newline. Verified only by CI (`rust-check` job) after push.
- **Local verification loop (after each phase):**
  `bash scripts/check-version.sh` → `npx vue-tsc --noEmit` → `npm test`.
  `npm run typecheck` = icon generator + vue-tsc; **every new `solar:*` icon
  must exist** in `@iconify-json/solar` or the build fails.
- **No new dependencies.** Shamir (GF(256)) and the cron parser are hand-rolled.
- Contracts that must hold: error prefixes (`auth:/rate_limited:/not_found:/
  unsupported:→501/too_large:/integrity:/network:/disk_full:/conflict:`),
  async `202 {jobId}` + poll, 404-before-401 (new REST segments go into
  `crates/web/src/security.rs:126 ROUTED_SEGMENTS`), secrets never echoed
  (`hasKey` pattern), honest limits.
- Append `worklog.md` (newest last) at the end of each phase; update
  `tests/frontend/transport-routes.test.ts` for every new transport-routed
  command.
- Push/watch CI only when asked (`push.sh --watch`).

---

# Phase 1 — Scheduler (cron) service — implemented; lifecycle verification pending

**Deliverable:** `.cybsh` scripts run on cron/interval schedules on desktop +
server; manageable in the Tasks panel; a `cron` shell verb on all transports.

## 1a. Schedule parsing — new `crates/os/src/schedule.rs`

- `pub enum ScheduleSpec { Cron { m, h, dom, mon, dow }, Every { n, unit } }`
- `parse(&str) -> Result<ScheduleSpec, String>` — 5-field cron (`* , - /`
  subsets), `every <n><s|m|h|d>`, aliases `@hourly/@daily/@weekly`.
- `next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>>` —
  minute-iteration with a sane search cap (skip if >5y → `None`).
- TS twin `src/utils/schedule.ts` (parse + human preview “next: in 4m”) —
  same twin pattern as `scene.ts`; unit-tested on both sides.

## 1b. Persistence — `crates/db/src/database.rs`

- Table `schedules` → `ScheduleRow { id, path, expr, enabled, description,
  created_at, last_fired_at, next_fire_at, last_run_id, run_on_boot }`.
- Table `schedule_runs` → `ScheduleRun { runId, scheduleId, started_at,
  finished_at, status, exit_hint, output_tail (≤64 KiB) }` pruned with
  `SCHEDULE_RUN_HISTORY_LIMIT = 20` (copy `save_sync_run`, db.rs:420).
- Accessors: `save_schedule/list_schedules/get/remove_schedule`,
  `save_schedule_run/list_schedule_runs` (+ `copy_table!` for snapshot).

## 1c. Daemon — new `crates/os/src/scheduler.rs`

Copies the canonical pattern of `crates/sync/src/scheduler.rs:34-65`:

- `static SCHEDULER: OnceLock<()>` + `ensure_started(db: Arc<RwLock<Database>>)`
  + named thread `cybermanju-cron`, 60 s tick.
- Tick: read enabled rows inside `db.read()` → due (`next_fire_at <= now`) →
  set `last_fired/next_fire` **before** running (no double-fire) → execute →
  write `schedule_runs` + `log_audit`.
- Execution = **existing entry, no 4th dialect**: load script bytes from the
  volume (same path the `run` verb uses, shell.rs:4330) →
  `cybermanju_os::script::run_source_with(...)` with wall-clock timeout
  (default 300 s) + output capture; failures recorded as `status: error` with
  AGENT-1-prefixed messages; the thread never panics.
- Missing script path → row status `not_found:`, tick continues.

## 1d. Wiring (the step that killed `ensure_scrub_started` — dead code today)

- Start hooks **both** places, next to the existing calls:
  `crates/web/src/lib.rs:856,887` and `src-tauri/src/commands/sync.rs:68,78`.
- **No thread in os-wasm.** Browser: store-level `tickSchedules()` (interval +
  on-panel-focus, gated `!document.hidden`) lists due rows via db op and runs
  them through `wasmOsDispatch('exec', …)`.

## 1e. REST family — `crates/web`

- New `api/cron.rs` with `route()` (pattern: `api/disk_api.rs:122`):
  - `GET /api/cron` (list) · `POST /api/cron` (create) · `PUT /api/cron/{id}`
  - `DELETE /api/cron/{id}` · `POST /api/cron/{id}/run` (inline, capped)
  - `POST /api/cron/{id}/enable|disable` · `GET /api/cron/{id}/runs`
- `security.rs`: add `"cron"` to `ROUTED_SEGMENTS` (else 404-before-401 eats
  the family). Roles: list/run/run-now = `Authenticated`; create/update/delete
  = `Admin` (a schedule runs arbitrary scripts → same posture as the P0-8 MCP
  gating decision).

## 1f. Tauri + WASM + frontend transport

- `src-tauri/src/commands/cron.rs`: `cron_list, cron_save, cron_delete,
  cron_run, cron_history, cron_set_enabled` → register in
  `src-tauri/src/lib.rs:379 generate_handler!`.
- `crates/os-wasm/src/db.rs`: ops `cron.list|save|delete|run|history|enable`.
- `src/composables/useTauri.ts`: `REST_ROUTES` entries, `REST_FIRST` (cron
  domain, like sync), `DB_WASM_ROUTES`, `MOBILE_NATIVE_OS_COMMANDS` (mobile
  has no server). Update `tests/frontend/transport-routes.test.ts`.

## 1g. cybsh verb `cron` (all 3 transports)

- Native `crates/os/src/shell.rs`: add `cron` to `command_table()` (:62) —
  subcommands `ls|add <path> [expr]|rm|run|enable|disable|history` with
  `--json`; `add` with no expr reads the file’s `# schedule:` frontmatter
  (`script::parse_frontmatter`, script.rs:414). Help + completion entries —
  `tab_completion_covers_the_command_table` (shell.rs:5433) auto-asserts.
- WASM `crates/os-wasm/src/os.rs` dispatch arm (db ops).
- Static `src/utils/staticCybsh.ts`: `HANDLED_VERBS` (:171) += `cron`.

## 1h. UI

- `ProcessPanel.vue` gains a **Schedules** tab (tab pattern from
  `OrganizePanel`): rows (script path, expr, next-fire countdown, last status),
  add/edit `UiModal` (path + expr + validated live preview), enable toggle,
  Run now, history drawer.
- Store: `schedules`, `fetchSchedules`, `cronSave/Delete/Run`, poll like
  `fetchOsJobs`.
- `src/utils/panels.ts` alias: `cron`/`automation`/`schedules` → `processes`
  with `{ tab: 'schedules' }` (`ALIAS_TAB_PROPS`).
- CommandPalette: “Schedules” entry → `wm.open('processes', {tab:'schedules'})`.

## 1i. Tests

- Rust: `schedule.rs` unit tests (cron next-fire with fixed instants, alias/
  interval, invalid exprs); `crates/tests/src/os.rs` route gating (create as
  viewer → 403; unauth → 404-before-401 contract); `cron` verb tests (`--json`,
  frontmatter `add`).
- Frontend: `tests/frontend/schedule.test.ts` (TS twin), static-cybsh `cron`
  verb tests (`fakeDeps`), transport-routes additions.

---

# Phase 2 — Agent exposure: `os_*` + `ui_*` tools — implemented; Rust CI pending

**Deliverable:** the agent runs the OS syscall surface (cybsh-only, no host
shell) and drives the UI (open panels, navigate, notify) on all transports.

## 2a. New tools (3)

| Tool | Params | Permission default | Plan agent |
|---|---|---|---|
| `os_exec` | `command` (one cybsh line, `--json` where supported) | **ask** (granular on `command`) | **deny** (joins config.rs:179-188 list) |
| `ui_open_panel` | `panel` (PanelType id), optional `tab`, optional `path` (files only) | allow | allow |
| `ui_notify` | `level` (info/success/warning/error), `message` | allow | allow |

- `os_exec` ≠ `bash`: it **never falls through to `sh -c`** (returns
  `unsupported: '<verb>' is not a cybsh verb`); the `-os` host flag stays
  refused (os-wasm os.rs:716 guard + native equivalent). Implementation reuses
  `cybermanju_os::shell::execute(line, Some(db))` after a command_table check —
  the same cybsh-first path as `tool_bash` (agent_api.rs:1402-1417).

## 2b. Rust schema/prompt (each change is asserted by a guard test)

1. `crates/agent/src/protocol.rs:24` `TOOL_NAMES` += 3 → **18**.
2. `protocol.rs:60-202` `tool_def()` entries — descriptions state transport
   limits + error prefixes (copy the `bash` def style, :114-122).
3. Count guards `protocol.rs:752,786`: `Some(15)` → `Some(18)`.
4. `agent_loop.rs:229-272` prompt bullets; `:437-448` name list in
   `every_prompt_tool_exists_in_the_schema`.
5. `config.rs:179-188` plan deny-list += `os_exec`; TS mirror `useAgent.ts:248`.
6. Defaults: `ensure_default_agent_permissions` (types/agent.rs:408) — leave
   `os_exec` out (ask by default); `ui_*` need no entry (allow). Presets
   `src/types/index.ts:1027-1047`: balanced gets `ui_*` allow.

## 2c. Native execution — `crates/web/src/api/agent_api.rs`

- `exec_tool` match (:1832-2001) += 3 arms:
  - `tool_os_exec` → command_table check + `cybermanju_os::shell::execute`
    (db + volume); output through `clean_output`/`TOOL_OUTPUT_CAP` (automatic).
  - `tool_ui_open_panel` → validate id against a **Rust allowlist**
    `UI_PANEL_IDS` (new const in `crates/agent` or `crates/types`), else
    `not_found:`; set `JobState.ui_request = {"op":"open","panel":…,"tab":…,
    "path":…}` + monotonic `ui_seq`; return `ok: opened panel '<id>'`.
  - `tool_ui_notify` → `{"op":"notify","level":…,"msg":…}`, return `ok: notified`.
- `JobState` (:935) += `ui_request: Option<serde_json::Value>` +
  `ui_seq: u64` — rides the existing snapshot → SSE/poll path
  (lib.rs:3182). Consumed-once on the frontend (dedupe by `ui_seq`).
- Subagent keep-list (agent_api.rs:3921-3936): **exclude** all 3 (UI actions
  are parent-scoped; `os_exec` inherits `deny`); update the
  `SUBAGENT TOOLSET` text (:3881-3884).

## 2d. Browser execution — `src/composables/useAgent.ts`

- `execLocalTool` cases: `os_exec` → `wasmOsDispatch('exec', {line})` (same
  refusal shapes as the `bash` case :857); `ui_open_panel`/`ui_notify` →
  direct `wm.open(resolvePanel(panel))` / `notify()` (no channel needed
  in-browser).
- `decideLocalTool` plan list (:248) += `os_exec`; `SUBAGENT_TOOLS` (:93):
  leave out.
- `AGENT_TOOL_META` (`src/utils/agentUi.ts:24`) += 3 entries — **valid
  `solar:*` names only**.

## 2e. Frontend reaction to `ui_request`

- New util `src/utils/uiRequests.ts`: `applyUiRequest(req, {wm, notify})` —
  pure, testable; validates the panel id against `resolvePanel` (panels.ts).
- Hook in `src/stores/app.ts:subscribeAgentJob` (:2115) + 1.5 s poll fallback
  (:2170): on snapshot with `ui_request`, call `applyUiRequest`.
- Panel-id parity: canonical array shared by `uiRequests.ts` and asserted
  equal to `resolvePanel`’s accepted ids in one test.

## 2f. UI surface

- `AgentPanel.vue` `AGENT_TOOLS` (:1003) += 3 (+ fix pre-existing drift: it
  lists 11 of 15 — add `self_research`, `skill_save`, `mcp_attach`,
  `repo_analyze` while touching it).
- Browser prompts ×2 advertise the new tools:
  `AgentPanel.vue:1468-1515 localSystemPrompt` and `useStudioAgent.ts:60-107`
  (also add the missing `question` there — known drift).

## 2g. Tests

- Rust: count guards (18), prompt-guard name list, `tool_os_exec` refusal
  shapes (`-os`, non-cybsh verb), `ui_request` set on job state.
- Frontend: `agent-wasm-parity.test.ts` (:105-113), `agent-ui.test.ts`, new
  `ui-requests.test.ts`, hermes strip tests if affected.

---

# Phase 3 — Secrets keystore (password manager) — integrated; security review pending

**Deliverable:** a Vault panel — store/search/generate/copy passwords and
notes; values sealed at rest; never echoed except on explicit reveal
(audit-logged); clipboard auto-clear; agent gets `secret_list` + gated
`secret_get`.

## 3a. Data + crypto core

- New table `secrets` (db.rs): key = id, JSON =
  `SecretRow { id, kind, title, username, url, category, tags[], notes,
  favorite, created_at, updated_at, value_sealed, has_value }` —
  **`value_sealed` never serialized in list responses** (clone the `hasKey`
  contract, agent_api.rs:107-116; never-leak test cloned from
  `crates/tests/src/agent.rs:99-110`).
- Sealing = existing `keystore::seal` / `open_sealed` (`seal:v1`, Argon2id
  m=19456,t=2,p=1 + ChaCha20Poly1305) under a per-transport
  `vault_passphrase()`:
  - native: `keystore::master_passphrase()` (env/file/auto-gen, keystore.rs:88);
  - wasm: session vault passphrase (held after unlock) via new os-wasm
    exports `seal_blob`/`open_blob` in `crates/os-wasm/src/crypto.rs` —
    **byte-identical `seal:v1` format** (argon2 + chacha already in os-wasm);
  - neither available → honest `unsupported:` (not silent).

## 3b. Business logic + 3 transports

- `crates/web/src/api/secrets.rs` (single source, used by Tauri + REST):
  `list` (metadata only) · `create` · `update` · `delete` ·
  `reveal` (opens sealed value; **writes `log_audit("secret.reveal")`**) ·
  `search(q)` (title/username/url/tags only — never the value).
- REST `/api/secrets` family + `ROUTED_SEGMENTS += "secrets"`; roles:
  list/search = `Authenticated`, create/update/delete/reveal = `Admin`
  (dangerous-op posture; adjust if single-user desktop complains).
- Tauri `src-tauri/src/commands/secrets.rs` (6 commands) + `generate_handler!`.
- WASM ops `secrets.list|create|update|delete|reveal` in `os-wasm/db.rs`
  (reveal takes the session passphrase client-side; never stored).
- Frontend transport sets + `transport-routes.test.ts`.

## 3c. Agent tools (2): `secret_list`, `secret_get`

- `secret_list` → names/metadata only, default allow.
- `secret_get {id}` → **default ask** (approval card shows the secret title,
  never the value); the result carries plaintext to the model — documented
  loudly in the tool description + prompt bullet (“only when the user asked
  you to use this credential”). Plan agents: deny both.
- Bumps `TOOL_NAMES` 18 → **20** (+ count guards, prompts ×3, AGENT_TOOL_META,
  AGENT_TOOLS).

## 3d. UI — new `SecretsPanel.vue`

- PanelType `secrets`; aliases `passwords`, `credentials` (+ check `vault`
  for collision with the AccountManager “Vault file” tab before using it —
  assert in `panel-aliases.test.ts`).
- Register: `MODULE_METADATA` (verify icon name in @iconify-json/solar),
  `useWindowManager` defaultSizes + component map, Dock pin, Kickoff (iterates
  MODULE_METADATA), CommandPalette, mobile launcher grid entry.
- Layout: category sidebar (Logins / Cards / Notes / API keys) · search ·
  list rows · detail card with **Reveal** (30 s auto-hide), **Copy** with
  **clipboard auto-clear** (new `src/utils/clipboard.ts`: `copySecret()` sets
  a 30 s timer and overwrites with `""`; fake-timer tests), edit/create
  `UiModal`, **password generator** (`src/utils/password.ts`: length/charset/
  entropy bits via `crypto.getRandomValues`, unit-tested + strength meter).
- Store: `secrets`, `fetchSecrets/createSecret/updateSecret/deleteSecret/
  revealSecret` with AGENT-1 error hints.

## 3e. Tests

- Rust: secrets never-leak (list/get never contain the value), audit row on
  reveal, wasm `seal:v1` byte-parity vs native format (same params).
- Frontend: password generator (charset/length/entropy), clipboard
  auto-clear, secrets list redaction shape, transport-routes.

---

# Phase 4 — Folder/disk encryption (file/batch core implemented; recursive workflow pending)

**Required outcome:** “Encrypt folder” encrypts/decrypts actual file bytes and
reports truthful progress/results, with verified storage and restore semantics.
The shared per-file and file-ID batch pipeline is implemented; recursive folder
jobs and trash/restore semantics below are not integrated. The Rust changes
still require CI validation, not an assumed completion checkmark.

## 4a. Shared real-encrypt core (implemented; Rust validation pending CI)

- `crates/web/src/api/batch.rs` now owns real AEAD encryption, authenticated
  decryption, sidecar persistence, read-back round-trip verification before
  source unlink, and explicit interrupted-transition recovery. Tauri delegates
  to it; REST adds file encrypt/decrypt, batch decrypt and admin-only keypair
  provisioning. Unknown algorithms/missing paths or keys are refusals, not
  encryption flags. KEM modes use the public half; symmetric/signing-seed
  modes open the sealed backend secret and use the existing separated-KDF +
  ChaCha20-Poly1305 core (not a claim that signatures themselves encrypt).
- Native data remains the existing `.enc` + `.enc.meta.json` format. The
  browser uses its vault format and honestly labeled browser algorithms;
  this is not evidence that a browser vault artifact equals a native sidecar.
- Browser round-trip/tamper/failure tests use Node's real ChaCha20-Poly1305
  behind a WASM-export shim. They do not execute a compiled Rust/WASM binary.
- This core does not implement recursive folder selection, a folder manifest,
  a recoverable-trash purge policy, a size-bounded async folder job, or secure
  erasure of filesystem snapshots/backups. Those remain separate work below.

## 4b. Recursive folder job (202 contract)

- `POST /api/batch/folder` `{ fileId, mode: encrypt|decrypt, purge: trash }`
  → validate on request thread → `RunRegistry::begin_owned` → spawned worker
  (exact pattern `sync_api.rs:658-693`) → poll (reuse the generic
  `/api/sync/jobs/{id}` poll if it fits, else a dedicated
  `/api/batch/jobs/{id}`; keep the **202+poll** contract either way).
- Worker: recurse children via `parent_index` (`list_by_parent`), per file:
  encrypt → verify → **purge plaintext to trash** → DB: node replaced by `.enc`
  node (`encrypted=true`, `encryption_algorithm`) → progress + per-file errors
  collected (one failure ≠ abort; report at end with prefixes).
  - **Step 0 of this phase:** read `database.rs::trash_file` /
    `restore_from_trash` to confirm byte semantics. If trash is DB-soft-delete
    with bytes in place, move bytes to a `.trash/` backing dir under the vault;
    the trash row records the original path for restore.
- Decrypt reverse: decrypt → verify → restore plaintext from trash (or rewrite
  in place) → drop `.enc` artifacts → remove trash row.
- Tauri: `encrypt_folder`/`decrypt_folder` wrapping the same core.
- WASM: honest `unsupported:` (fits `WRITE_ONLY_COMMANDS` semantics — state it
  in ShieldPanel limits copy).

## 4c. Disk (“driver”) encryption honesty

- Research: disk blocks are **already sealed at rest** (`volume.rs`
  `encode_payload` → compress → `keystore::seal` → `CYBE1`). Work is truth,
  not crypto:
  - `DiskManagerPage.vue:325,346`: fix overstated key-holder copy
    (“🔑 holder — unwraps the others” → real meaning: single attached
    passphrase holder; link to Recovery).
  - Add a “sealed at rest ✓” badge (df/check card) sourced from real superblock
    state; dead `Superblock.key_handle` field: wire it to the recovery key
    handle (Phase 5) or leave + comment — decide at impl.

## 4d. UI

- FileManager context menu (folder rows): “Encrypt folder…” / “Decrypt
  folder…” → confirm `UiModal` (explains trash safety + size estimate) →
  progress via the job poll (reuse SyncPanel progress strip pattern).
- ShieldPanel “Shield” tab: folder-encryption status section (N files at
  rest, last job).

## 4e. Tests

- Rust (`crates/tests/src/`): folder encrypt round-trip on a temp tree
  (plaintext purged to trash, `.enc` valid, decrypt byte-identical), partial
  failure reporting, never-abort-on-one-bad-file.
- Frontend: context-menu wiring (light), transport-routes.

---

# Phase 5 — Recovery (file + Shamir 2-of-3 + trusted-device) — not integrated

**Proposed deliverable:** lost master passphrase recoverable via (a) `recovery.cybrec`
+ recovery passphrase, or (b) any 2 of 3 printed Shamir shares, plus
(c) trusted-device re-wrap for migration. `docs/SECURITY.md` §4 rewritten.

## 5a. Shamir — new `crates/crypto/src/shamir.rs` (hand-rolled, no deps)

- GF(256) arithmetic (AES polynomial 0x11b), Lagrange interpolation,
  `split(secret: &[u8], k: u8, n: u8) -> Vec<Vec<u8>>`,
  `combine(shares) -> Result<Vec<u8>>`.
- Encoding: versioned text `CYBRC1-<idx>-<total>-<k>-<base64>` with BLAKE3
  checksum; display groups for printing.
- Property tests: any k-of-n reconstructs; any k−1 is indistinguishable from
  random (entropy assertion); wrong checksum/index rejected.

## 5b. Recovery artifact

- Format `recovery.cybrec` = `keystore::seal(recovery_passphrase,
  master_passphrase_bytes)` (`seal:v1` — identical on wasm via Phase-3
  `seal_blob/open_blob`). Contents: nothing else.
- **Wizard** (prefer a ShieldPanel “Recovery” tab 3; decide at impl):
  1. set recovery passphrase (×2, strength meter),
  2. generate + download file (warn: “without this file or 2 shares the vault
     is unrecoverable”),
  3. optional: generate 3 Shamir shares of the same blob → render/print/export
     (confirm ≥2 stored before marking enabled),
  4. status flags in kv `recovery.meta` `{file: bool, shares: 2-of-3,
     created_at}` — `hasKey`-style, never the secret itself.
- Never-sync: `isProtectedSyncPath` (`src/utils/vaultFolder.ts`) += file name;
  `crates/sync/src/backends.rs:401` blocklist += same; add `recovery.cybrec`
  (+ note about `keystore.json`) to `scripts/backup.sh` includes — flagged in
  OPERATIONS (today a DB backup alone can’t unseal).

## 5c. Recover flow

- `POST /api/recovery/unseal` {mode: file|shares, payload…} → open `seal:v1`
  with the recovery passphrase → returns **the original master passphrase to
  the session** (never to a store): native writes `master.passphrase`
  (0600) + re-derives; wasm sets session passphrase → vault unlocks →
  `log_audit("recovery.used")`.
- Follow-up “rotate after recovery” (re-wrap `keystore.json` handles under a
  new passphrase): attempt in-phase if handle iteration is clean, else
  documented as next PR (SECURITY §4.1 gap — honest note).
- Roles: `create-file`/`unseal` = **Admin**, rate-limited (recovery is an
  online passphrase oracle → count failures, backoff — reuse security.rs
  helpers).
- Transports: REST `/api/recovery/*` + `ROUTED_SEGMENTS += "recovery"` ·
  Tauri `recovery_status/create_file/shares/unseal` · WASM
  `recovery.status|create|unseal` (browser file-import picker).

## 5d. UI

- Recovery wizard (create) + “Lost your passphrase?” entry from
  ServerAuthPanel/landing boot → file picker or paste-2-shares → recovery
  passphrase → unlock. Status chips wherever the vault is described
  (“recoverable: file ✓ · shares 2/3”).

## 5e. Tests + docs

- Rust: shamir properties, recovery round-trip (create → unseal → identical
  passphrase), rate-limit on bad attempts, never-leak on status endpoints.
- Frontend: wizard step logic, share-code parse/format, status chips.
- **Docs:** rewrite `docs/SECURITY.md` §4 (threat model: attacker needs file
  OR 2 shares + passphrase), `docs/OPERATIONS.md` backup section,
  `AGENTS.md` §5 contract notes, `README` feature line, `worklog.md` entry.

---

# Execution order & verification cadence

| Phase | Ends with | Expected CI feedback |
|---|---|---|
| 1 Scheduler | `check-version.sh` (if touched) + `vue-tsc` + `npm test` + worklog | schedule.rs parser tests, shell verb parity, cron route gating |
| 2 Agent tools | same + transport-routes | `Some(18)` guards, prompt guard, refusal tests |
| 3 Secrets | same + never-leak test | seal:v1 parity, secrets leak tests |
| 4 Folder crypto | same + round-trip test | trash-purge round-trip, partial-failure |
| 5 Recovery | same + SECURITY.md rewrite | shamir properties, recovery round-trip, rate-limit |

- Rust written blind per AI.md conventions each phase; if CI `rust-check`
  fails, fix-forward on the next push (never local cargo).
- After each phase the user decides: push + `push.sh --watch`, or continue to
  the next phase first.
- Final doc-accuracy pass: AGENTS.md/README counts (tools 15→20, commands,
  route arms) — currently stale anyway.

# Key risks / verify-during-implementation

1. **Trash byte semantics unknown before Phase 4** — first action of that
   phase: read `database.rs::trash_file` + `restore_from_trash`; adapt purge.
2. **Three tool-count guard sites + three prompt copies** (Rust ×2, TS ×2,
   AGENT_TOOLS) — missing one fails a build or silently hides a tool.
3. **New REST families must land in `ROUTED_SEGMENTS`** or they 404 before auth.
4. **Panel alias collisions** (`vault`, `keys`) — check `resolvePanel` + tests.
5. **Icon names** must exist in `@iconify-json/solar` (typecheck gate).
6. **`ensure_scrub_started` precedent** — wiring `ensure_started` in *both*
   lib.rs and commands/sync.rs is the easiest step to forget.
7. **Browser prompt drift already real** (`question` missing) — fix while
   editing both TS prompts.
