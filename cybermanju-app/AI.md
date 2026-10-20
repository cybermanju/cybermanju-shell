# AI — AI Agent inference system (review + fixes)

Living task tracker for the agent review and the P0/P1 fixes. Status legend:
`[ ]` todo · `[~]` in progress · `[x]` done · `[-]` dropped/deferred

Companion doc: **`docs/AGENT-REVIEW.md`** — the full review (system map,
defects D1–D9, UX gaps G1–G10, opencode insight table, backlog P0–P3).
This file is the tracker; that file is the reasoning.

**Constraints (locked in):**
- **NEVER install or run Rust/Cargo locally.** All `cargo check` / `cargo test` /
  clippy runs in **GitHub CI** on push (`.github/workflows/ci.yml`). Rust changes
  are written blind against docs.rs and verified only in CI.
- Local verification loop = `npm run typecheck` (runs `npm run icons` first — an
  unknown icon name **fails the build**, so every `solar:*` name must exist in
  `@iconify-json/solar`) + `npx vitest run`.
- Never assume a library is available; check `package.json` first.
- P2/P3 items stay out of this pass unless explicitly promoted.

**Baseline:** `npm run typecheck` ✅ · `npx vitest run` 18/18 ✅ (3 files)
→ **after this pass: 40/40 ✅ (4 files)** · Rust: unverified locally, CI only.

**Current reconciliation (2026-10-20):** this file's counts and Phase 0–3
test totals are historical, not the current repository baseline. The planned
`os_exec`, `ui_open_panel`, `ui_notify` and `secret_list`/`secret_get` tools
are now present across the Rust/browser agent paths and UI metadata; see
`PLAN.md` for evidence and remaining phases. This documentation pass did not
run tests; Rust/security status remains **unverified pending CI**, and the
aggregate P0 closure is reopened in `TASKS.md` while concurrent fixes proceed.

---

## Phase 0 — P0 correctness (inference-side defects)

- [x] 0.1 **D1 — `question` was advertised in the prompt but missing from the tool
      schema** (providers refuse calls for names they were not shown).
      `crates/agent/src/protocol.rs`: added `"question"` to `TOOL_NAMES` + a
      `question` `tool_def` (`required: ["question"]`); count assertions
      `Some(8)` → `Some(9)`; renamed the shape test to
      `tool_schemas_cover_nine_tools_in_openai_shape` and asserts `question`.
- [x] 0.2 **D1 guard** — `crates/agent/src/agent_loop.rs`:
      `every_prompt_tool_exists_in_the_schema` fails the build if the prompt
      ever advertises a tool `TOOL_NAMES` omits again.
- [x] 0.3 **D3 — context overflow misclassified as `network:`** (the UI hint for
      `network` says "retry", which cannot work).
      `protocol.rs::classify_provider_error` branches on
      `context_length_exceeded` / `prompt is too long` / `maximum context` /
      `context window` / `too many tokens` / … → `context:`; test
      `overflow_is_context_not_network` also asserts plain 500s stay `network:`.
      Prompt + `agentErrorHint('context')` both say **COMPACT**.
- [x] 0.4 **D2 — browser `ALLOW ALWAYS` dropped `remember`** (the button did
      nothing on the WASM transport, so the ask came back every time).
      `useAgent.ts`: `ApprovalRequest.resolve(approved, answer, remember?)`,
      `LocalRunOpts.onRemember`, `rememberAllowLocal()` (mirror of Rust
      `config::remember_allow` — writes `rules[tool] = 'allow'`, never an
      invisible always-list); `AgentPanel.answerApproval` passes `remember`
      through; `sendPromptLocal.onRemember` persists the config and toasts the
      exact rule written.
- [x] 0.5 **D5a — no doom-loop guard in the browser loop.** `runLocalAgent`
      counts byte-identical `(name, input_json)` triples across the whole run
      and answers `denied: identical tool call repeated 3 times (doom-loop
      guard)`.
- [x] 0.6 **D5b — native guard only counted within one tool batch**, so a model
      looping once per turn (the common case) was never caught. Hoisted
      `doom_sig`/`doom_repeats` out of the `ToolCalls` arm in
      `crates/web/src/api/agent_api.rs` so the counter spans the run — which is
      what the system prompt already promises ("three identical repeats are
      auto-denied").
- [x] 0.7 **D5c — `expected_hash` was ignored by the browser `edit` tool**
      (native refused stale writes; the browser silently clobbered).
      `applyEditLocal` is async and verifies BLAKE3 via
      `wasmModuleExports().blake3_hash` when the module is up, answering
      `integrity: file changed since anchor …` on mismatch; `execLocalTool`
      returns the new `blake3:` so the next edit can anchor.
- [x] 0.8 **D6 — agent errors reused the sync error hints.** New
      `agentErrorHint()` in `src/types/index.ts` maps the house machine
      prefixes (`auth`/`rate_limited`/`context`/`not_found`/`unsupported`/
      `too_large`/`integrity`/`conflict`/`invalid`/`network`) to the next human
      step; `AgentPanel.jobHint` uses it instead of `describeSyncError`.

- [x] 0.9 **D9 — `expected_hash` was unusable end-to-end.** `read` returned raw
      content with no hash, and both `write` and `edit` printed only a **16-char**
      prefix while `apply_edit` demanded the full 64-char hex — so every anchor the
      model could obtain failed `integrity:`. Fixed as one contract:
      · `crates/agent/src/edit.rs` — `anchor_line()` + `strip_anchor()` (a trailing
      `[blake3:<hex>]` line is metadata, never file content), and `apply_edit`
      accepts a **unique prefix** (empty = no anchor).
      · `agent_api.rs` — `read_raw` (internal) + `tool_read` (appends the anchor);
      `tool_write` strips the echoed trailer and prints the **full** hash; the edit
      arm hashes exactly the bytes written.
      · prompt + schema now describe the trailer; browser mirrors it
      (`anchorLineLocal`/`stripAnchorLocal`, prefix check in `applyEditLocal`) and
      `localSystemPrompt` documents it.
      · `apply_edit` normalizes whatever shape the model echoes back (`blake3:`,
      brackets, whitespace).
      · Rust tests: `read_anchor_round_trips_and_never_survives_a_write`,
      `a_short_anchor_prefix_still_verifies`; JS: 2 `stripAnchorLocal` tests.

## Phase 1 — P1 interface (the agent must be legible)

- [x] 1.1 **`src/utils/agentUi.ts` (new)** — transport-agnostic thread helpers:
      `AGENT_TOOL_META`/`toolMeta`, `salientArg` (same precedence as Rust
      `config::salient_arg`), `toolTitle`, `toolStateFromResult` (classifies by
      machine prefix), `buildThread` (pairs `assistant_tool` with its `tool`
      result, keeps the lead text, marks unanswered calls `running` while the
      run is live and `error` when it is not, and surfaces a result whose
      assistant block never arrived), `estimateTranscriptTokens` (~4 chars/token),
      `contextWindowFor`, `estimateCost` (null when the model is unknown),
      `toolPermissions` (same `decide` the loop applies), `permissionLabel`.
- [x] 1.2 **`src/utils/markdown.ts` (new)** — escape-first renderer for
      assistant text (`v-html`-safe): fenced code lifted out before escaping,
      headings/hr/quote/lists/paragraphs, links restricted to
      `http(s)/mailto/#//` so `javascript:` never survives.
- [x] 1.3 **D4 — frozen thread during a run.**
      · native: `JobSnapshot.activity` + `JobState.activity` (`crates/web/…/agent_api.rs`),
        set to `thinking · <model>` before the provider hop, `<tool> <salient arg>`
        per call, `waiting for your approval` / `waiting to spawn a subagent`
        while parked, cleared on finish/failure.
      · panel: re-reads the session every 1.5 s while the job is live
        (`refreshLiveThread`), and the WASM loop mirrors its session into the
        viewer on every `onUpdate`.
- [x] 1.4 **Capability surface** (top of the panel): transport · model · kind ·
      working dir · ruleset label · key state, plus a per-tool chip showing the
      **same `decide` the loop will apply** (`ALLOW`/`ASK`/`DENY`) and, in the
      browser, `NO SHELL` for `bash`/`task` with an explicit "browser sandbox"
      note.
- [x] 1.5 **Thread rendering**: tool rows grouped under their lead text with an
      icon, one-line title, state (spinner / `DENIED` / `ERROR`), expandable
      input+result; assistant messages rendered as markdown; a turn footer on
      the last assistant message (`AGENT · model · KIND · n msgs · CTX ~… · ~$…`).
- [x] 1.6 **Context meter**: `CTX EST n / window (p%)` bar (green→amber→red at
      60/85 %), provider-reported `IN/OUT`, and an approximate `~$` cost — the
      raw token counters are gone.
- [x] 1.7 **Approval card shows the contract**: tool, salient target, the raw
      input, and the literal rule `rules["<tool>"] = "allow"` that **ALLOW ALWAYS**
      writes; `ALLOW ONCE` / `ALLOW ALWAYS` / `DENY` plus the "denials are
      information" note.
- [x] 1.8 **Message queue**: sending while a run is live queues the prompt and
      shows a `QUEUED n` strip with per-item drop; the queue drains when the job
      reaches a terminal state. Button reads `QUEUE` while running.
- [x] 1.9 **Job line**: `JOB xxxx… · RUNNING · TURN 3/25 · read src/app.rs · 42s · 2 QUEUED`
      (activity + elapsed clock), with `agentErrorHint` on failure.
- [x] 1.10 **`src/stores/app.ts`** — one-shot notifications when a job hits
      `waiting_approval` ("needs approval — <tool>"), `done`, `error`, `cancelled`.
- [x] 1.11 **`StatusBar.vue`** — `AGENT:WAIT` (amber, pulsing) / `AGENT:3/25` /
      `AGENT:IDLE`, tooltip carries the live activity, click opens the panel.
- [x] 1.12 **`CommandPalette.vue`** — AGENT group: open panel · abort current run
      · analyze repo (writes AGENTS.md).

## Phase 2 — tests + docs

- [x] 2.1 `tests/frontend/agent-ui.test.ts` — 22 tests: thread pairing/running/
      denied/orphan results, prefix classification, salient titles, token
      estimate, window/cost, capability surface + `rememberAllowLocal`,
      markdown escaping (`<img>`/`<script>` never survive, `javascript:` links
      dropped, fences escaped), `agentErrorHint`, browser `stripAnchorLocal` + `normalizeAnchorLocal`.
- [x] 2.2 `npm run typecheck` ✅ · `npx vitest run` **40/40 ✅**
- [x] 2.3 `docs/AGENT-REVIEW.md` — the full review + opencode insight table.
- [x] 2.4 **Rust written blind + hardened against `cargo fmt --check` and
      `clippy -D warnings`** (CI runs both — `.github/workflows/ci.yml`), verified
      by hand against the existing code's formatting patterns, never by running a
      toolchain locally:
      · every new code line ≤ 100 cols (the only >100 lines are pre-existing
      section comments and string literals rustfmt cannot split);
      · multi-method chains stay on one line only at ≤ 60 cols (`chain_width`),
      otherwise split one `.method()` per line — measured against the repo (longest
      single-line chain in fmt-clean code is exactly 60);
      · long literals moved to the module const `DOOM_LOOP_DENIAL` so the denial
      call site fits, matching the `DECRYPT_ERROR` / `HINT` const shape;
      · `if`-condition breaks put `{` on its own line (pqc.rs precedent);
      · `match`-on-`Option` early-return mirrors the pre-existing
      `fuzzy_locate`/`first` shape (clippy-clean); no `map_or(false, …)`;
      · no trailing whitespace, no consecutive blank lines, one EOF newline.
- [ ] 2.5 **Rust verified in CI only** — push and watch `rust-check`
      (`fmt`, `clippy -D warnings`, `cargo test`: `protocol.rs` schema tests,
      `agent_loop.rs` prompt tests, `edit.rs` anchor tests,
      `agent_api.rs` activity/doom-loop/read-anchor/write-strip).

## Phase 3 — P2/P3 (landed 2026-10-06)

- [x] 3.1 P2 — approval **diff view** (`src/utils/agentDiff.ts` + approval card:
      `diffBlocks`/`editBlocksOf`, 6 vitest).
- [x] 3.2 P2 — **reject-with-feedback**: DENY carries `denyReason` into
      `denied: … — user feedback: …` on both transports (native `approve`
      `answer`, browser `waitApproval` answer).
- [x] 3.3 P2 — **granular permission editor** (per-tool ALLOW/ASK/DENY selects
      writing the same ruleset the loop enforces; SAVE RULES persists).
- [x] 3.4 P2 — **strip denied tools from the schema** (`strip_denied_tools`
      in `crates/agent/src/config.rs`, called in `agent_api.rs` ×2; TS mirror
      `stripDeniedTools` in `agentUi.ts`).
- [x] 3.5 P2 — **SSE streaming** of assistant text (native
      `GET /api/agent/jobs/:id/events` via fetch reader in
      `store.subscribeAgentJob`, `parseAgentStreamChunk` + 6 vitest, poll fallback).
- [x] 3.6 P2 — `LimitReached` → actionable "turn budget exhausted — raise MAX
      TURNS or COMPACT" (`AgentPanel` limit copy + `agentErrorHint('limit')`).
- [x] 3.7 P3 — auto-compaction when the context meter passes ~85 %
      (`shouldAutoCompact` + once-per-session watcher, old transcript kept).
- [x] 3.8 P3 — attention surface (pulsing `.approval.attention` + `AGENT:WAIT`
      status while `waiting_approval`).
