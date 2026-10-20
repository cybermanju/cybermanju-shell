# Semantic Memory — how the agent remembers

Two layers, one contract: **verbatim transcripts** (what happened) plus
**semantic memory** (what was learned). Transcripts compact; memory persists.

> Scope lock: no Rust/Cargo on this box — Rust items are CI-only
> (`.github/workflows/ci.yml`). Local verification is
> `npm run typecheck` + `npx vitest run`
> (`tests/frontend/memory.test.ts` pins the TS half of the shared contract).

---

## 1. How memory works today

| Layer | Store | Code |
|---|---|---|
| Full transcripts | redb `agent_sessions` (JSON rows, same DB file) | `crates/db/src/database.rs` (`AGENT_SESSIONS_TABLE`), `crates/web/src/api/agent_api.rs` (`persist_turn`, `list/get/create/import_session`) |
| Browser transcripts | localStorage `cybermanju.agent.sessions.v1` (cap 100) | `src/composables/useAgent.ts` |
| Wire pruning | `prepareWireMessages`: tool outputs cut at 85 % ctx, transcript untouched | `src/utils/agentUi.ts` |
| Compaction | LLM summary → fresh session (old kept); handoff ALSO stored as memory | `compact_session` (+ CompactHandoff hook) |
| **Semantic memory** | redb `agent_memories` (text + vectors as JSON rows) | **this doc** — everything below |

Before this change, recall was verbatim-only: every prompt re-sent full
history, and nothing survived compaction except the one handoff message.
Hermes (below) showed the missing half: bounded, curated, cross-session
memory with its own retrieval.

## 2. What Hermes does in Python (researched 2026-10-06)

Nous Research `hermes-agent` (Python, MIT) — the reference our harness
mirrors (`docs/HERMES-INSIGHTS.md`):

- **Files, not tables**: primary cross-session memory lives in
  `~/.hermes/memories/` as `MEMORY.md` + `USER.md`; SQLite + FTS5 session
  search is a *separate* retrieval layer. Our mapping: redb rows are the
  store of record; `GET /api/agent/memories/export` renders the
  Hermes-compatible `MEMORY.md` companion (see §5).
- **Bounded by config**: `memory.memory_char_limit: 2200` (~800 tokens),
  `user_char_limit: 1375` (~500 tokens). Our mapping: identical constants —
  `MEMORY_RECALL_BUDGET_CHARS = 2200` (`crates/agent/src/memory.rs`,
  `src/utils/memory.ts`).
- **Write approval**: `memory.write_approval` gates memory writes (staged →
  `/memory approve`). Our mapping: `memory_remember` goes through the same
  `decide()` permission gate as every tool (default `ask`), and
  standing-orders protection applies to its content via `salient_arg`
  (`text`).
- **Closed learning loop**: agent-curated memory with periodic nudges,
  autonomous skill creation, FTS5 recall + LLM summarization. Our mapping:
  auto-recall injection, compact-handoff auto-store, terminal-state
  `memory_hint` nudge (§4).
- **Pluggable backends**: built-in, Honcho (dialectic user modeling), Mem0,
  OpenViking, Holographic, RetainDB, ByteRover, Supermemory. Our mapping: the
  `AgentMemory` row is backend-agnostic (text + optional vector); swapping the
  embedder partitions by dims instead of corrupting (§3).

## 3. Rust vector-storage research (why redb + brute force)

Evaluated October 2026 — embedded-first (no sidecar server), pure-Rust
preferred (matches the workspace: redb, tantivy, no C++ toolchain):

| Crate | Shape | Verdict |
|---|---|---|
| `instant-distance` (djc) | Pure-Rust HNSW, MIT/Apache, prod-used (InstantDomainSearch), `Builder` + `Search` | **Phase-2 pick** for ANN rebuild-from-redb if rows ever exceed ~10k |
| `hnswlib-rs` / `hnsw` | Pure-Rust HNSW graph decoupled from storage | Viable alternative to instant-distance; revisit in phase 2 |
| `vecstore` ("SQLite of vector search": HNSW + filters + hybrid + ONNX) | All-in-one, very young, huge claimed surface | **Rejected** — too immature to trust with the DB file |
| `ruvector-core` (HNSW + redb mmap) | Closest to our stack on paper | **Rejected** — new org, unproven; redb already covers our durability |
| `usearch` | Native bindings (C++), fastest | **Rejected** — breaks the pure-Rust build (esp. WASM-facing crates) |
| `lance` / `qdrant-client` | Columnar format / server client | **Rejected** — heavyweight; qdrant needs a server process |
| Tantivy (already vendored) | BM25 full-text, no vectors | Keeps its job: file search. Memory gets vectors, not a second index |

**Decision: exact brute-force cosine in `crates/agent/src/memory.rs`, vectors
persisted in redb.** At 1536 dims × 10k rows a recall is ~60M flops —
single-digit milliseconds, no index to corrupt, no migration to run. The
crossover where HNSW beats brute force (~5–20k points, measured by the
from-scratch `HNSW-Vector-Search` project) is far above personal-memory
scale (hundreds of rows).

Embeddings come from the **provider's OpenAI-compatible `/embeddings`**
(`{base}/embeddings`, model per `AgentConfig.embeddingModel`, default
`text-embedding-3-small`) — zero new dependencies, works with OpenAI,
Ollama (`nomic-embed-text`, fully offline), OpenRouter, and any OpenAI-clone.
Anthropic has no embeddings API: honest `unsupported:` → keyword fallback
(see §4). Local ONNX (MiniLM via the existing `ort` dependency in `faces`)
stays a phase-2 option; it was not wired to avoid model-weight downloads in
the default path.

## 4. Design

### Record (`crates/types/src/agent.rs`)

`AgentMemory { id, configId, text, embedding: f32[], dims, origin, sessionId?,
uses, createdAt, updatedAt }` — `origin: remember | compactHandoff | import`.
`dims` is the comparability gate: recall only compares same-dims vectors, so
swapping the embedding model starts a fresh partition instead of corrupting
ranking. Vectorless rows (embed failed, Anthropic dialect) are **keyword-only,
never dropped**.

### Hybrid recall (`crates/agent/src/memory.rs`, mirrored in `src/utils/memory.ts`)

- Vector cosine (f64 accumulation) when the query embedding is comparable and
  scores `>= 0.30`; otherwise keyword overlap (lowercase alnum tokens,
  `len >= 3`, `>= 2` overlap) scoring `0.25 + 0.05 × overlap` capped at
  `0.69` — above noise, below a solid vector match, so the scales never
  masquerade as each other.
- Ties break by `uses` (bumped per served recall), then recency.
- `top_k` clamped 1–10, default 3. Token rule, formula, and budgets are
  **identical on both sides of the FFI** and pinned by
  `tests/frontend/memory.test.ts` + `memory.rs` unit tests.

### Wiring (native)

- **Auto-recall**: `run_agent_job` embeds the user prompt and prepends a
  bounded `<recalled-memories>` block to the system prompt. Empty store or
  failed embedding → byte-identical prompt; memory augments, never blocks.
- **Tools** (schema + prompt + `decide` gate, stripped when denied like
  everything else): `memory_recall {query, top_k?}` (read-only),
  `memory_remember {text}` (one fact per call; secrets redacted via
  `redact::redact`, capped at 2000 chars, `invalid:` when empty).
  Subagents get `deny:` on remember (they report; the parent stores) —
  mirroring the existing subagent mutation gate. `salient_arg` covers `text`,
  so content patterns (`*password*`) match what would be stored.
- **Compact hook**: the compaction handoff is auto-stored
  (`origin: compactHandoff`, session-scoped) — future recalls find what the
  compacted session learned although its transcript is gone.
- **Nudge**: terminal states set `JobSnapshot.memoryHint` when
  `turns >= 8` with no remember call — one "teach me" hint, never mid-run
  nagging (Hermes periodic-nudge parity).
- **Budgets**: recall block 2200 chars, note cap 2000 chars, list cap 500
  newest, output truncation follows the existing 64 KiB tool cap.

### Browser (WASM) parity

No embeddings endpoint in the sandbox: `LocalMemoryStore`
(`src/utils/memory.ts`, localStorage `cybermanju.agent.memories.v1`, cap 500)
recalls keyword-only over the same rank/score contract, results marked
`degraded:`; `sendLocal` prepends the block to the browser system prompt and
`memory_recall`/`memory_remember` execute against it. Imported rows that carry
vectors (via export envelopes) rank by vector when comparable. The WASM tool
schema comes from Rust `protocol::tool_definitions()` — rebuilt with the wasm
bundle in CI, so offering follows automatically.

### Sync + `.cybermanju` mapping

Memories live in the same redb file as sessions (the volume home) and travel
the same path: `GET /api/agent/memories/export` returns
`{ markdown, memories }` — Hermes-compatible `MEMORY.md` plus full rows
**with vectors** for restore; import re-keys through the store endpoint
(`origin: import` available for migrations). No separate sidecar file to lose.

## 5. API reference

| Surface | Call |
|---|---|
| REST | `GET /api/agent/memories?configId=&vectors=` · `POST /api/agent/memories {configId, sessionId?, text}` → 200 row · `DELETE /api/agent/memories/:id` · `POST /api/agent/memories/recall {configId?, query, topK?}` (`configId` omitted = keyword-only global search) · `GET /api/agent/memories/export?configId=` |
| Tauri | `list_agent_memories`, `store_agent_memory`, `recall_agent_memories`, `delete_agent_memory` (+ REST fallbacks in `REST_ROUTES`) |
| Store | `agentMemories`, `fetchAgentMemories`, `storeAgentMemory`, `recallAgentMemories`, `deleteAgentMemory` (`src/stores/app.ts`) |
| Agent tools | `memory_recall`, `memory_remember` (native + browser) |
| Job | `memoryHint?: string \| null` on `AgentJob` / `JobSnapshot` |

Auth follows the existing `agent` segment rules; mutating memory writes go
through permission `decide()` (default `ask`); keys never leave the server
(`sync_secrets agent:key:<id>`, memory-only in the browser).

## 6. Verification

| Check | Command | Result |
|---|---|---|
| Types | `npm run typecheck` | 0 errors |
| Unit tests | `npx vitest run` | green incl. `tests/frontend/memory.test.ts` (19 tests: cosine, token contract, chunking, rank parity, budgets, nudge, local store) |
| Rust | CI only | `memory.rs` unit tests (cosine, tokens, chunks, vector-vs-keyword ranking, dim-mismatch fallback, tiebreaks, block budgets, export order, nudge, sanitize); `protocol.rs` 9→11 tool assertions; `agent_loop.rs` prompt↔schema parity incl. memory tools; `config.rs` `text` salient-arg |

## 7. Phase 2 (not this pass)

- `instant-distance` HNSW rebuilt from redb rows if stores exceed ~10k rows.
- Local ONNX embeddings (MiniLM via existing `ort`) for a zero-network path.
- `memory.write_approval`-style staged writes (`pending/` review) à la Hermes.
- AgentPanel memory surface (list / recall preview / one-tap REMEMBER from
  `memoryHint`) + per-config embedding-model picker.
- `.cybermanju/MEMORY.md` volume-file mirror (export writes it next to
  `AGENTS.md`, like `ai init`).
