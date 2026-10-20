# Hermes Insights → CyberManju

How the **Hermes agent-harness patterns** (skills · memory · permissions ·
sessions · gateway) map onto CyberManju OS, what already matches, and what
this pass implemented on the TS side.

> Scope lock: no Rust/Cargo on this box — Rust items are CI-only
> (`.github/workflows/ci.yml`). Local verification is
> `npm run typecheck` + `npx vitest run`. TS changes below are covered by
> `tests/frontend/hermes.test.ts`.

Hermes here means the reference behaviour our code already mirrors in comments:
`approvals.denial_breaker_threshold` (3), `security.protected_instruction_files`,
`proactive_prune` (no-LLM prune before summarization), early compression, and the
standing-orders guard that holds even under `--yolo`. Evidence in-tree:
`src/composables/useAgent.ts` (DenialBreaker, protectedAutoApproveDenial,
prepareWireMessages call-site), `src/utils/agentUi.ts` (WirePruneOptions),
`crates/agent/src/config.rs` (is_protected_instruction_path).

---

## 1. Skills — standing orders + composable capabilities

### Hermes pattern

- Skills are **versioned instruction packs**: a manifest (`name`, `description`,
  `tools`, `version`) plus a markdown body. Discovery scans well-known paths,
  the manifest decides loading, the body is injected into the system prompt.
- **Standing orders are protected**: whoever can rewrite the instruction files
  owns every later turn, so writes to them are approval-gated *even in yolo
  mode*, and deny still beats auto-approve.
- Budgets apply: per-file and total caps so a hostile or bloated skill cannot
  eat the context window.

### CyberManju mapping

| Hermes | CyberManju | File |
|---|---|---|
| Skill manifest + body | Project rules loaded as three fixed paths (`AGENTS.md`, `SKILL.md`, `.cybermanju/rules.md`, 8 KiB each / 24 KiB total) | `docs/OPERATIONS.md` §4b; native loader (Rust, CI-only) |
| `protected_instruction_files` | `is_protected_instruction_path` (TS) / `config::is_protected_instruction_path` (Rust) — case-insensitive basename in any dir + `.cybermanju/rules.md` form | `src/composables/useAgent.ts:175`; `crates/agent/src/config.rs:24` |
| Yolo-proof guard | `decideLocalTool` downgrades `write`/`edit` on protected paths from `allow` → `ask`; `protectedAutoApproveDenial` fails AUTO-APPROVE closed with a `deny:` the model can act on | `src/composables/useAgent.ts:142`, `useAgent.ts:186` |
| Skill-aware prompt | `localSystemPrompt` documents the sandbox + anchor contract | `src/composables/useStudioAgent.ts:46` |
| `ai init` writes `AGENTS.md` with the agent's own tools | OPERATIONS §4b | docs |

### Gaps found

1. **No manifest** — TS knew three hardcoded basenames but could not parse a
   `SKILL.md` frontmatter (`name`/`description`/`tools`/`version`), so it could
   not rank, select, or attribute skills.
2. **No TS-side budget assembly** — the 8 KiB / 24 KiB contract lived only in
   docs + Rust; the browser loop had no pure assembler to enforce it.
3. **No relevance selection** — every skill was always injected; Hermes selects
   by goal overlap.

### Implemented this pass (TS, tested)

New pure module `src/utils/skills.ts`:

- `parseSkillFrontmatter(text)` — tolerant `---` frontmatter parse (`name`,
  `description`, `version`, `tools` csv/list); body returned separately; no
  frontmatter → `{ manifest: null, body: text }`, never throws.
- `assembleStandingOrders(files)` — enforces `SKILL_FILE_BUDGET = 8 KiB` per
  file and `SKILL_TOTAL_BUDGET = 24 KiB` total (mirrors OPERATIONS §4b); emits
  a `<standing-orders>` block; truncated files carry a `truncated:` marker so
  the machine-prefix contract still classifies them.
- `selectSkills(skills, goal, limit)` — deterministic keyword-overlap ranking
  over `name + description` (ties broken alphabetically); the prompt keeps the
  top-N instead of everything.

---

## 2. Memory — transcripts, pruning, compaction

### Hermes pattern

- **Transcripts are append-only memory**; the UI keeps every byte while the
  *wire* gets a pruned copy (`proactive_prune`: a no-LLM first pass that cuts
  tool outputs before any summarization is attempted).
- Compression starts **early** (well below overflow), keeps head/tail of tool
  results, and marks cuts with a machine prefix so the model knows the text is
  partial.
- Explicit compaction (summarize → fresh session, old kept for revert) is the
  recovery path for `context:` overflow — retrying an overflowed transcript can
  never work.

### CyberManju mapping

| Hermes | CyberManju | File |
|---|---|---|
| `proactive_prune` | `prepareWireMessages` — cuts only `role: 'tool'` results over 1200 chars to head 400 + tail 200, keeps newest 12 messages, prefixes `truncated:`; transcript array never mutated | `src/utils/agentUi.ts:381`; call-site `src/composables/useAgent.ts:514` |
| Early compression | Default threshold 85 % of `contextWindowFor(model)` + 64-token headroom so one pass lands under the line | `src/utils/agentUi.ts:385` |
| `context:` overflow contract | `classify_provider_error` → `context:`; `agentErrorHint('context')` says COMPACT; system prompts say compact-then-continue | `src/types/index.ts:580` |
| Server compact | `POST …/compact` summarizes into a fresh session, old kept | `docs/OPERATIONS.md` §4b |
| Turn persistence | Jobs persist turns; crash never loses the transcript | `docs/AGENT-REVIEW.md` §2 |

### Gaps found

1. **No declarative auto-compact trigger** — the 85 % number was buried as a
   default arg; UI/P1 backlog (auto-compaction at ~85 %) had no pure predicate
   to build on.
2. **No fork primitive** — compact/fork/revert flows need a pure session fork
   (new id, message slice, usage handling); only ad-hoc copies existed.

### Implemented this pass (TS, tested)

New pure module `src/utils/sessionMemory.ts`:

- `shouldAutoCompact(pct, threshold = AUTO_COMPACT_PCT = 85)` — single predicate
  the future auto-compact watcher can call; boundary-inclusive, NaN-safe.
- `forkSession(session, { idFactory, keepLast, resetUsage, titleSuffix })` —
  pure fork for compact/revert flows; usage optionally reset, title suffixed.
- `exportSession` / `importSession` — versioned (`SESSION_EXPORT_VERSION = 1`)
  JSON round-trip with structural validation; import never throws, it returns
  `{ ok, session?, error? }`.
- `sessionTitleFor(prompt)` — first-8-words title fallback (mirrors the
  existing `sendLocal` behaviour, now unit-pinned).

### Semantic memory v2 (implemented — see `docs/MEMORY.md`)

Session transcripts were verbatim-only recall. The semantic layer adds what
Hermes has in Python (`MEMORY.md`/`USER.md`, 2200-char budget, write
approval, FTS5 + LLM summarization, periodic nudges):

- redb `agent_memories` table (same DB file): curated text + embedding
  vectors, namespaced per config and per dims (model swaps partition, never
  corrupt); vectorless rows stay keyword-recallable.
- Hybrid recall (vector cosine ≥ 0.30 else keyword `0.25 + 0.05×overlap` capped
  0.69; `uses`/recency tiebreak) in `crates/agent/src/memory.rs`, mirrored
  exactly in `src/utils/memory.ts` (shared test contract).
- `memory_recall` / `memory_remember` tools (schema + prompt + `decide` gate;
  subagents denied on remember); pre-prompt auto-recall block (2200 chars);
  compact-handoff auto-store; terminal-state `memoryHint` nudge.
- Embeddings via provider `{base}/embeddings` (Ollama works offline);
  Anthropic degrades to keywords honestly. REST + Tauri + store actions;
  browser loop recalls keyword-only (`degraded:`) over a localStorage store.

Deliberately **not** implemented: LLM summarization itself (server-side,
CI-only by scope lock).

---

## 3. Permissions — decide, deny-wins, yolo-proofing

### Hermes pattern

- One decision vocabulary: `allow | ask | deny`, evaluated per `(tool, input)`
  with last-match-wins wildcard patterns over the tool name, the `tool + salient
  arg` target, and the bare arg (`git *` matches `git status`, prefix-free).
- **Deny beats auto**; plan persona denies mutations unconditionally; standing
  orders never auto-allow (ask-downgrade even under yolo).
- `approvals.denial_breaker_threshold` (3): three consecutive human `no`s on one
  tool open the breaker — the loop stops parking on the human and tells the
  model to take a different approach.
- Denied tools are **stripped from the schema** so the model stops attempting
  what it cannot do (opencode behaviour, AGENT-REVIEW D7/P2).

### CyberManju mapping

| Hermes | CyberManju | File |
|---|---|---|
| `decide` | `decide` (Rust) / `decideLocalTool` (TS mirror) — identical semantics incl. plan-deny, last-match-wins, protected ask-downgrade | `crates/agent/src/config.rs:164`; `src/composables/useAgent.ts:115` |
| Wildcards/globs | `matchWildcard`/`matchGlob` mirrors, parity-pinned | `src/composables/useAgent.ts:32`; `tests/frontend/agent-matchers.test.ts` |
| `remember_allow` | `rememberAllowLocal` — "allow always" becomes visible `rules[tool] = 'allow'`, never an invisible list | `src/composables/useAgent.ts:162` |
| `denial_breaker_threshold` | `DenialBreaker` (threshold 3, per-tool counts, approval clears) wired into both ask paths | `src/composables/useAgent.ts:198` |
| Doom-loop guard | Byte-identical `(name, input_json)` ×3 → `denied:` across the whole run (both transports) | `src/composables/useAgent.ts:548` |
| Capability surface | `toolPermissions` + `permissionLabel` render the same `decide` the loop applies | `src/utils/agentUi.ts:431` |

### Gaps found

1. **D7 open**: denied tools stayed advertised in the schema, so the model kept
   trying them (deferred P2 in AGENT-REVIEW).
2. **No human-readable audit line** — the decision had no one-line explanation
   for logs/approval cards beyond `ask`/`deny`.

### Implemented this pass (TS, tested)

In `src/utils/agentUi.ts` (reuses the loop's own `decideLocalTool`, so the
surface cannot drift from enforcement):

- `stripDeniedTools(tools, rules, kind)` — returns the tool list minus anything
  `decideLocalTool(…, {})` denies (plan-kind drops `edit`/`write`/`bash`
  automatically). First TS step of D7: the schema builder can pass its tool
  list through this before advertising.
- `explainDecision(rules, kind, tool, input)` — one-line audit string
  (`ALLOW read /src/a.ts — default allow`, `ASK bash "rm -rf /" — needs
  approval`, `DENY … — denied by the permission ruleset`), reusing the salient
  arg so the log reads like the job line.

Full schema-stripping on the native transport stays Rust-side (CI-only).

---

## 4. Sessions — jobs, approvals, lifecycle

### Hermes pattern

- Sessions own the transcript + usage; **jobs are detached runs** over a
  session (async: `prompt_async` → poll → approve/abort), so the UI never blocks
  on inference.
- Asks **park** (Hermes: ~10 min) then auto-deny; approvals resolve
  once/allow-always/deny; terminal states re-sync the transcript.
- Lifecycle ops: create, load, delete, **fork**, **export/import** (portable
  JSON), GC/caps so history cannot grow unbounded.

### CyberManju mapping

| Hermes | CyberManju | File |
|---|---|---|
| Detached jobs | `POST /api/agent/prompt → 202 {jobId}`, poll `GET /api/agent/jobs/{jobId}`, approve/abort; browser mirror `runLocalAgent` with `LocalRunOpts` + `ApprovalRequest` parking | `docs/OPERATIONS.md` §4b; `src/composables/useAgent.ts:435` |
| Ask parking | 10-min server park then auto-deny; browser `waitApproval` promise parked in `pendingApproval` ref, abort resolves it `false` | docs; `src/composables/useAgent.ts:473` |
| Live transcript | `activity` on every interesting point + 1.5 s `refreshLiveThread` (native) / `onUpdate` mirror (WASM) | `src/composables/useStudioAgent.ts:404` |
| Queue | Prompt-while-running queues, drains on terminal, `QUEUED n` strip | `src/composables/useStudioAgent.ts:386` |
| Bounded history | localStorage sessions capped at 100, newest-first | `src/composables/useAgent.ts:699` |
| Secrets hygiene | Keys sealed server-side (`sync_secrets agent:key:<id>`), memory-only in browser, never persisted (`hasKey: false` on save) | `src/composables/useAgent.ts:686` |

### Gaps found

1. **Export/import unvalidated** — no versioned, validated session envelope;
   a corrupt import could crash the viewer.
2. **Fork ad-hoc** — no shared pure helper (now `forkSession`, see §2).

### Implemented this pass

`exportSession` / `importSession` / `forkSession` in
`src/utils/sessionMemory.ts` (tested: round-trip, version rejection, corrupt
payloads, oversized transcripts stay intact — import validates shape, never
content length).

---

## 5. Gateway — one interface over every transport

### Hermes pattern

- The **gateway** is the transport edge: one client shape over local IPC,
  HTTP/SSE, and remote-attach, with environment detection, auth injection,
  health, and honest `unsupported:` answers where a transport cannot do
  something (no fake Ok).
- Retries apply to `network:` / `rate_limited:` (bounded, backoff); `auth:`
  never retries; `context:` redirects to compact.

### CyberManju mapping

| Hermes | CyberManju | File |
|---|---|---|
| Tri-mode gateway | `useTauri.ts`: Tauri IPC (`invoke`) ↔ REST (`fetch` → `:3456`, JWT bearer, 401 → `cybermanju:unauthorized`) ↔ WASM volume dispatcher; `isStaticHost` routes static origins to WASM instead of a dead `localhost:3456` | `src/composables/useTauri.ts:81`, `useTauri.ts:97`, `useTauri.ts:128` |
| REST contract | `REST_ROUTES` maps IPC names → method/path + snake→camel response fixup | `src/composables/useTauri.ts:210` |
| Dual agent drivers | Server loop (`startAgentRun` + poll) vs browser loop (`runLocalAgent` volume tools) behind `useStudioAgent` | `src/composables/useStudioAgent.ts:1` |
| Honest sandbox | `bash`/`task`/`question` answer `unsupported:` in the browser with the fix (desktop/Docker) named | `src/composables/useAgent.ts:422` |
| Retry posture | Rate-limit backoff (3 retries), abort-checked retries (native, per OPERATIONS §4b) | docs |

### Gaps found

1. **Transport choice untestable** — `isStaticHost`/`getBaseUrl` read
   `window`/module state directly; no pure resolver to pin the matrix.
2. **Backoff unpinned** — the 3-retry posture had no pure schedule function.
3. **Error→action mapping split** — `agentErrorHint` knew the hint but nothing
   pure decided retry vs redirect.

### Implemented this pass (TS, tested)

New pure module `src/utils/gateway.ts`:

- `resolveTransport({ isTauri, serverUrl, port })` — pure mirror of the
  `isTauri → rest : rest-same-origin : wasm-static` decision; truth table in
  tests.
- `backoffMs(attempt, baseMs = 300, capMs = 5000)` — deterministic exponential
  schedule (`300 · 2^attempt` capped); jitter stays at the call site so the
  schedule is unit-pinnable.
- `classifyGatewayError(message)` — maps text to `auth | rate_limited |
  network | context | unknown` using the same substring signals as
  `classify_provider_error` (context phrases) + HTTP 401/429 markers.
- `shouldRetry(kind)` — retries `network`/`rate_limited` only; `auth` and
  `context` never retry (context redirects to COMPACT per `agentErrorHint`).

---

## Verification

| Check | Command (in `cybermanju.github.io/`) | Result |
|---|---|---|
| Types | `npm run typecheck` | must be 0 errors |
| Unit tests | `npx vitest run` | must be green (incl. new `tests/frontend/hermes.test.ts`) |
| Rust parity | CI only (`cargo fmt --check`, `clippy -D warnings`, `cargo test`) | untouched this pass — no Rust files changed |

## Backlog (not this pass)

- **Rust/CI:** native schema-stripping of denied tools (full D7), granular
  per-pattern permission editor UI (AI.md 3.3), SSE streaming, actionable
  `LimitReached`, server-side skill manifest loading + `load_project_rules`
  parity with `assembleStandingOrders`.
- **P3:** auto-compaction watcher built on `shouldAutoCompact` (meter > 85 %),
  per-tool latency/cost history, attention surface while `waiting_approval`.
- **Gateway:** SSE tail through the hand-rolled HTTP server (chunked streaming
  first; polling stays the fallback — see OPENCODE-PORT risk register).
