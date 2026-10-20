# Porting opencode into CyberManju OS — research + port plan

Researched 2026-10-06 against **opencode 1.18.34** (installed locally, server
probed live on `:4199`: `/global/health` → `{healthy:true}`, 162-path OpenAPI
3.1 spec at `/doc`, 479 KiB). Verdict up front: **do not port the binary —
integrate at its API boundary.** opencode is already a client/server system;
our OS just becomes another client, with our Kernel as its sandbox.

## 1. What opencode actually is (verified, not assumed)

- **TypeScript/Bun server + Go TUI.** `opencode serve` (default `:4096`,
  `--cors`, `--hostname`, basic-auth via `OPENCODE_SERVER_PASSWORD`) exposes
  the full product over HTTP; the TUI/desktop/VSCode clients are thin.
  Dual namespaces in 1.18.x: legacy (`/session`, `/tui`, `/file`, `/find`,
  `/provider`, `/mcp`, `/event`) + new `/api/*` (162 paths total).
- **Typed SDK = fetch.** `@opencode-ai/sdk` (`createOpencodeClient({baseUrl})`)
  is a thin fetch wrapper over the OpenAPI spec — it runs anywhere our
  frontend runs, no Bun needed on the client side.
- **Second protocol: ACP.** `opencode acp` speaks Zed's Agent Client Protocol
  (JSON-RPC over stdio) — the escape hatch if HTTP ever doesn't fit.
- **Agent loop primitives that match our OS 1:1:** sessions CRUD,
  `prompt` (blocking) / `prompt_async` (204) + `session/active` polling,
  `abort`, permission-request reply, `todo`, `diff`/`share`/`revert`/`fork`,
  SSE (`/event`, `/global/event`), PTY (`/api/pty`), MCP connect, LSP status,
  `PUT /auth/{providerID}` (credential injection), per-session `permission`
  rulesets, `task` subagents, `question` tool.
- **Sandbox model we can steal:** per-tool `allow|ask|deny` + wildcard
  patterns + `external_directory` confinement + per-agent overrides +
  `--auto` flag. `read` denies `*.env` by default.
- **Storage:** `~/.local/share/opencode/opencode.db` (SQLite sessions),
  `repos/`, `snapshot/`, `tool-output/`; `export/import` session JSON.
- **Hard limits found by probing:** binary is **177 MB** (Bun-compiled,
  self-contained, no runtime to install); cwd at server start = project root;
  inference needs provider keys (none present here — all flow below was
  verified up to the auth boundary, not a live completion).

## 2. Why a literal port is wrong

| Approach | Fate |
|---|---|
| Compile opencode to WASM | Dead on arrival: Bun runtime + Node APIs + SQLite + PTY + subprocess have no browser target. |
| Rewrite the agent loop in Rust | Burns the model/provider/MCP/LSP ecosystem; we would re-own prompt caching, retries, diff application forever. |
| Bundle 177 MB into every installer | Triples download size for a feature half the users won't enable. |
| **Sidecar + SDK client (chosen)** | opencode runs unmodified next to us; our Vue frontend speaks HTTP exactly like the TUI does. Upgrades = swap one binary. |

## 3. Target architecture (per transport)

```
┌─ Tauri desktop ──────────────────────────────┐  ┌─ Docker ───────────┐
│ Vue AgentPanel ──fetch──▶ opencode serve     │  │ dashboard :3456     │
│  (localhost, basic-auth)   ├─ cwd = volume   │  │  ├─ /api/llm/* proxy│
│ cybsh `ai` ───────────────┘    root          │  │  └─ sidecar layer   │
│ Kernel syscalls = tool sandbox               │  │     (opt-in image)  │
└──────────────────────────────────────────────┘  └─────────────────────┘
┌─ Pages/WASM ─────────────────────────────────┐
│ AgentPanel ──▶ attach home daemon (URL in    │  (no subprocess in
│ Settings)  OR  dashboard LLM proxy (Phase 2)  │   browser — by design)
└──────────────────────────────────────────────┘
```

- **Desktop:** Tauri shell plugin (`shell.open: true` already configured)
  spawns `opencode serve --port <free> --hostname 127.0.0.1` with cwd set to
  the merged-volume root and env seeded from our keystore
  (`ANTHROPIC_API_KEY`, … — secrets never touch disk or logs). `--cors`
  allowlists only our origin. Binary is an **opt-in download** (fetch +
  hash-verify on first enable), not a bundled sidecar.
- **Docker:** optional image layer adds the same binary; dashboard owns its
  lifecycle. Works headless via `prompt_async` + polling.
- **WASM:** two honest modes — (a) **remote attach** today (`opencode attach`
  semantics; Settings already shows transport + server URL), (b) **dashboard
  LLM proxy** later (`POST /api/llm/*` on `:3456` forwards with
  keystore-held keys, solving browser CORS the same way our REST layer
  already solves auth).

## 4. Contract mapping (opencode ⇄ our OS)

| opencode concept | Our counterpart | Wire-up |
|---|---|---|
| `permission` ruleset per session | RBAC roles + `auth:`/`unsupported:` error prefixes | Generate from role: `viewer` → plan agent, `edit: deny`; `user` → ask-by-default; `admin` → allow within volume. `external_directory: deny` always (volume root is the world). |
| `ask` permission requests | `cybermanju:unauthorized`-style event → login-popup pattern | Poll `GET /session/:id/permission` or subscribe `/event` SSE; approve/deny UI reuses the permission panel; `--auto` only behind an explicit admin toggle. |
| Tool `bash/read/edit/glob/grep` | Kernel (`open/read/write/seek/close/stat/unlink/readdir/mkdir`) | Constrain cwd to volume root; `bash` allowlist mirrors our quota/rate-limit posture (`rm *`-class patterns stay `deny`). |
| `task` subagents | `compute.workers()` (local rayon + provider slots) | Each subagent = one `TaskTable` row via the repair-bridge pattern (`REPAIR_ID_BASE` mirror); `ps`/`kill` work unchanged; provider count visibly grows parallelism. |
| Sessions in `opencode.db` | Striped placement + `sync_files` | Nightly `export` → content-addressed chunks across providers (`whole` default, `striped` opt-in); restore on a new machine replays history. Cross-device continuity without trusting any one provider. |
| `PUT /auth/{id}` credentials | `keystore::{seal,open_sealed}` + `sync_secrets` side table | Provider keys sealed at rest, merged at spawn time, never serialized to clients (same rule as sync tokens). |
| Lease / single writer | `lease::acquire_lease_in(volume)` | One active agent writer per volume; second device gets `conflict:`, resolved through existing `ConflictPolicy`. |
| Versions | `versions::create` before every agent edit batch | Same hook the editor's save path already uses — agent edits stay undoable. |
| `tree-sitter` vs our code intel | Our `parse_text` + outline | Feed `extract_symbols` output as repo-map context; opencode's own LSP stays authoritative for edits. |
| `cybsh` | `ai` command | `ai "<prompt>" [--config X] [--model p/m]` → `prompt_async`, prints `{jobId}`-style session id; `sync status`-style polling already exists; output streams via SSE like `compute run` tails logs. |

## 5. New surfaces (all additive, all honest on failure)

- `src/components/AgentPanel.vue` (lazy, like TerminalPanel): session list,
  prompt box with `@`-file attach (reuse file fuzzy finder), model/agent
  picker from `GET /config/providers`, live SSE event tail, permission
  approve/deny cards, abort, token/cost readout from `stats`.
- `src/composables/useOpencode.ts`: minimal fetch client (subset of the
  SDK shapes — sessions, messages, permissions, events, health; or npm
  `@opencode-ai/sdk` directly since it is fetch-only).
- `crates/web/src/api/agent_api.rs`: sidecar lifecycle
  (spawn/health/kill, free-port pick, password mint), permission-ruleset
  generation from RBAC, `POST /api/llm/*` proxy (Phase 2), session-export
  scheduling into striped placement.
- `crates/os/src/shell.rs`: `ai`, `ai-sessions`, `ai-abort` commands via the
  same lockless-intercept pattern as `sync start`.
- Settings: provider-key vault UI (sealed entries, per-key provider select),
  sidecar status (version/health/port), attach-URL field for Pages mode.

## 6. Explicit non-goals (documented, not deferred silently)

- No WASM build of the agent runtime itself (physically impossible — see §2).
- No remote arbitrary code execution on providers (compute fan-out stays a
  constrained task ABI, per MISSING.md).
- No bundled 177 MB binary in the default install path.
- No silent auto-approve: `ask` defaults win unless an admin opts into auto
  mode per session, and every approval lands in the audit log with identity.

## 7. Phased delivery

- **Phase 0 (days):** `useOpencode.ts` + `AgentPanel.vue` against a
  manually-started `opencode serve`; keystore-held keys exported by hand;
  hardcoded ask-by-default ruleset. Proves the loop end to end.
- **Phase 1 (sidecar):** `agent_api.rs` lifecycle + generated rulesets +
  `cybsh ai`; Docker layer; session-export-to-striping; audit wiring.
- **Phase 2 (untethered web):** `/api/llm/*` proxy for Pages; MCP server we
  expose (kernel ops) so opencode reasons over the merged volume from any
  cwd; `acp` evaluated as the stdio alternative for the desktop spawn.
- **Phase 3 (depth):** subagent→provider-slot scheduling, repo-map from our
  search index, lease-scoped multi-device sessions, cost dashboards from
  `stats`/`export`.

*Risk register: opencode 1.x API is pre-1.0-stable in places (`/api/*` vs
legacy namespaces both served in 1.18.x — pin `1.18.x` in the sidecar
downloader and re-verify `/doc` on bump); SSE through our hand-rolled HTTP
server needs chunked-streaming support before the event tail can be live
(polling `session/active` is the fallback); 351 MB local opencode data dir
observed here argues for session-export GC from day one.*
