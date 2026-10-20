# AI Agent inference system — review

Full review of the agent stack (native REST job + browser/WASM loop), the
insights borrowed from the **opencode** repo, and the prioritized backlog.
Tracker with per-task status: **`AI.md`**.

**Verification constraints (locked):** no Rust/Cargo on the dev box — every
`cargo check`/`cargo test` runs in GitHub CI on push. Local loop is
`npm run typecheck` + `npx vitest run`.

---

## 1. System map

| Layer | File(s) | Responsibility |
|---|---|---|
| Wire protocol | `crates/agent/src/protocol.rs` | Tool schemas (9), request builders (OpenAI + Anthropic), reply parsers, **error classification** (`auth:`/`rate_limited:`/`context:`/`network:`) |
| Turn machine | `crates/agent/src/agent_loop.rs` | System prompt, turn loop, `ToolCalls`/`TextDone`/`LimitReached` events, repo overview |
| Permissions | `crates/agent/src/config.rs` | `decide`, `match_wildcard`/`match_glob`, `salient_arg`, `remember_allow` |
| Native driver | `crates/web/src/api/agent_api.rs` | Job lifecycle, approvals, MCP merge, subagents, doom-loop guard, persistence |
| Browser driver | `src/composables/useAgent.ts` | Same shapes over `fetch` + WASM volume tools, localStorage configs/sessions |
| Store | `src/stores/app.ts` | Providers/configs/sessions, 1.5 s job poller, notifications |
| UI | `src/components/AgentPanel.vue` | Setup wizard, configs, MCP, sessions, thread, approvals, prompt |
| UI helpers | `src/utils/agentUi.ts`, `src/utils/markdown.ts` | Thread grouping, titles, context/cost, capability surface, safe markdown |
| Types | `src/types/index.ts` | `AgentJob`(+`activity`), `ChatMessage`, `agentErrorHint`, permission presets |
| Surfaces | `src/components/StatusBar.vue`, `CommandPalette.vue` | Global agent visibility + commands |

## 2. What is already good

- **One permission semantics, two transports.** `config::decide` and
  `decideLocalTool` are mirrors, and `tests/frontend/agent-matchers.test.ts`
  pins wildcard/glob/grep parity. Denials carry a machine prefix the model is
  told to work around, not retry.
- **Honest failure prefixes everywhere** (`not_found:`/`conflict:`/
  `integrity:`/`unsupported:`) — the model gets actionable, structured errors
  instead of prose.
- **`expected_hash` anchors** on native edits, and `edit` refuses ambiguous
  blocks (`conflict:`) rather than guessing.
- **Subagents are gated twice**: the request schema is stripped to
  `read|list|grep` *and* a runtime gate answers `deny: subagents cannot run …`.
- **Secrets never round-trip** — keys are sealed server-side and memory-only in
  the browser.
- **Bounded work**: `maxTurns`, timeouts, output redaction/truncation, turn
  persistence so a crash never loses the transcript.

## 3. Defects

| # | Severity | Defect | Evidence | Status |
|---|---|---|---|---|
| **D1** | **High** | `question` was advertised in the system prompt but missing from the tool schema — providers reject calls for names they were never shown, so the "ask the human" path was dead. | `protocol.rs` `TOOL_NAMES` had 8 entries, prompt documented `question` | **fixed** (schema + `every_prompt_tool_exists_in_the_schema` guard) |
| **D2** | **High** | Browser `ALLOW ALWAYS` dropped `remember` — `answerApproval(approved, remember)` never forwarded it to `pending.resolve`, so "always" behaved like "once" on WASM. | `AgentPanel.vue` wasm branch of `answerApproval` | **fixed** (`remember` threaded through, `rememberAllowLocal` + config persist + toast of the exact rule) |
| **D3** | **High** | Context overflow classified as `network:`, whose UI hint says "retry" — the one thing that cannot work. | `classify_provider_error` had no context branch | **fixed** (→ `context:`, hint says COMPACT; test pins both directions) |
| **D4** | **High** | The thread froze for the whole run: no activity field, no live transcript. `JobSnapshot` had no `activity`, the panel only reloaded the session on terminal status. | `agent_api.rs` `JobSnapshot`; `AgentPanel` status watcher | **fixed** (`activity` set at every interesting point; 1.5 s live session read; WASM mirrors on `onUpdate`) |
| **D5** | **High** | Doom-loop guard existed only *within one tool batch*, so a model repeating one call per turn burned the entire turn budget; the browser loop had no guard at all, and its `edit` ignored `expected_hash` (silent clobber). | `agent_api.rs` `ToolCalls` arm; `useAgent.ts` `applyEditLocal` | **fixed** (counters hoisted across the run in both transports; BLAKE3 anchor honored when wasm is up) |
| **D9** | **High** | `expected_hash` was advertised in the prompt *and* the schema but **unusable end-to-end**: `read` returned no hash, and `write`/`edit` printed a 16-char prefix while `apply_edit` demanded the full 64-char hex — every obtainable anchor failed `integrity:`. | `agent_api.rs` `tool_read`/`tool_write`; `edit::apply_edit` | **fixed** — `read` appends `[blake3:<hex>]`, `write`/`edit` strip the echoed trailer and print the full hash, `apply_edit` accepts a unique prefix, prompt+schema+browser mirror agree (D9) |
| **D6** | Medium | Agent failures reused **sync** error hints — "check your connection" for a `context:` overflow. | `AgentPanel.jobHint` → `describeSyncError` | **fixed** (`agentErrorHint` maps the 11 machine prefixes) |
| **D7** | Medium | Denied tools stay in the schema, so the model keeps trying them. | `protocol.rs` / subagent strip | deferred (**P2** — opencode removes them) |
| **D8** | Low | `LimitReached` → bare "turn budget exhausted" with no next step. | native terminal message | deferred (**P2**) |

## 4. Interface gaps (before → after)

| # | Gap | Before | After |
|---|---|---|---|
| **G1** | Tool activity | a static `TOOL: …` block per message, results detached from calls | grouped rows: icon + one-line title, spinner while running, `DENIED`/`ERROR` badge, expandable input + result |
| **G2** | Context awareness | raw `TOKENS IN/OUT` counters | `CTX EST n / window (p%)` bar with green/amber/red at 60/85 %, provider `IN/OUT`, approximate `~$` |
| **G3** | What the agent may do | invisible until something was denied | **capability surface**: transport · model · kind · working dir · ruleset label · key state + per-tool `ALLOW`/`ASK`/`DENY` evaluated by the very `decide()` the loop uses |
| **G4** | Browser limits | discovered by calling `bash` | `bash`/`task` chips read `NO SHELL` with an explicit sandbox note |
| **G5** | Approval | summary + buttons, effect of "always" unknown | tool, salient target, raw input, and the literal `rules["<tool>"] = "allow"` that **ALLOW ALWAYS** writes |
| **G6** | Blocked-agent visibility | nothing outside the panel | one-shot notifications on `waiting_approval`/terminal + `AGENT:WAIT` (amber pulse) in the status bar |
| **G7** | Prompt while running | send disabled | queue with a `QUEUED n` strip, drain on terminal, button reads `QUEUE` |
| **G8** | Assistant text | `white-space: pre-wrap` plain text | markdown (escape-first, `v-html`-safe) |
| **G9** | Job status | `JOB xxxx… · RUNNING · TURN 3/25` | + `read src/app.rs` activity, elapsed clock, queued count |
| **G10** | Reachability | panel only | command palette: open panel · abort run · analyze repo |

## 5. What opencode does that we borrowed

Report from the opencode repo (see `docs/OPENCODE-PORT.md` for the wider port).

| opencode behaviour | Why it matters | Our state |
|---|---|---|
| **Permission modes `once`/`always`/`reject`** | one decision vocabulary; `always` is durable, `once` is scoped to the call | ✅ `ALLOW ONCE` / `ALLOW ALWAYS` / `DENY`, `always` persists as an explicit rule |
| **Reject is feedback, not silence** | a refusal should change the next attempt | ✅ denial text tells the model to work around it; ⏳ **P2** free-text reject reason |
| **Denied tools stripped from the schema** | the model stops attempting what it cannot do | ⏳ **P2** (D7) |
| **Inline tool rows + QUEUED badge** | the transcript reads as a log, not a stack dump | ✅ `buildThread` rows + queue strip |
| **Context ring / token budget** | overflow is visible *before* it becomes an error | ✅ context meter + `context:` hint |
| **Per-turn footer (`model · kind · duration`)** | each answer carries its own cost | ✅ turn footer with model, kind, msgs, CTX, ~$ |
| **Auto-compaction on overflow** | the session survives its own success | ⏳ **P3** (COMPACT button exists, automatic does not) |
| **Attention/notifications when blocked on the user** | the human is the bottleneck, so surface it | ✅ toasts + `AGENT:WAIT` + tooltip activity |
| **MCP status surfaced** | invisible servers = invisible failures | ✅ MCP section with `LIST TOOLS` + attached servers |
| **SSE dual-granularity streaming** | typing, not turn-by-turn pops | ⏳ **P2** |
| **Doom-loop / external-directory as first-class permissions** | models fail predictably and cheaply | ✅ doom-loop guard in both transports; ⏳ external-directory rule is n/a to our volume model |
| **Per-turn footer (`model · kind · duration · interrupted`)** | attribution without a stats page | ✅ turn footer |

## 6. Backlog

**P0 — correctness (done this pass):** D1, D2, D3, D4, D5 (a+b+c), D6, D9.
**P1 — interface (done this pass):** G1–G10.

**P2 — next pass**
1. Approval **diff view** (pre/post) instead of raw JSON.
2. **Reject with feedback** — DENY carries a reason the model must obey.
3. **Strip denied tools from the schema** (D7) + expose a granular
   per-tool/per-pattern permission editor writing the same ruleset.
4. **SSE streaming** of assistant text on the native transport.
5. **Actionable `LimitReached`** (D8): "raise MAX TURNS or COMPACT".

**P3 — later**
6. Auto-compaction when the meter passes ~85 %.
7. Attention surface (panel flash/pulse while `waiting_approval`).
8. Per-tool latency/cost history on the capability surface.

## 7. Verification

| Check | Command | Result |
|---|---|---|
| Types | `npm run typecheck` (runs `npm run icons`) | ✅ 144 icon names resolved, 0 errors |
| Unit tests | `npx vitest run` | ✅ **40/40** (22 new in `tests/frontend/agent-ui.test.ts`) |
| Rust | `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` | ⏳ **CI only** — `edit.rs` (anchor round-trip, prefix, normalization), `protocol.rs` (9-tool schema + classification), `agent_loop.rs` (prompt/schema parity), `agent_api.rs` (activity, doom-loop hoist, read anchor, write strip) |

Written blind, so the Rust edits were hardened by hand against the two CI gates
that are not type errors:

- **`cargo fmt --check`** — every new code line ≤ 100 cols; multi-method chains
  only on one line at ≤ 60 cols (measured against this repo: the longest
  fmt-clean single-line chain is exactly 60), otherwise split one `.method()`
  per line; `{` goes on its own line after a broken `if` condition; long
  literals live in a module const (`DOOM_LOOP_DENIAL`, matching
  `DECRYPT_ERROR`/`HINT`); no trailing whitespace, no double blank lines, one
  newline at EOF. String literals inside `json!`/`format!` stay untouched (the
  pre-existing >100-col lines are all comments or those literals).
- **`clippy -D warnings`** — no `map_or(false, …)` (the repo never uses it),
  `match`-on-`Option` with an early `return` mirrors the pre-existing
  `fuzzy_locate` shape, no needless borrows, no `if let` chains that would want
  `let…else`.

Risks to watch on the first CI run: the `doom_sig`/`doom_repeats` borrow shape in
`agent_api.rs`, `TOOL_NAMES.contains(&tool)` inference in the new `agent_loop`
test, the `format!` continuation in the subagent system prompt, byte-index
slicing in `strip_anchor`, and any rustfmt opinion my hand-formatting missed —
`cargo fmt --check` runs first in `rust-check`, so it fails fast if so.
