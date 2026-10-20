# AI agent alternatives for CyberManju OS — research + recommendation

Researched 2026-10-06. Question: what gives this OS AI coding ability on
**all three transports** (Tauri desktop, Docker web, Pages WASM) over
**decentralized storage**, without repeating opencode's 177 MB sidecar?
See also `docs/OPENCODE-PORT.md` (sidecar integration, still valid for
desktop/Docker power users).

## 1. Candidates found

### oh-my-pi (`can1357/oh-my-pi`, MIT, ~14.6k stars) — closest fit, borrowable
- Fork of Mario Zechner's Pi. TS+Bun CLI plus a **~55k-line Rust core**;
  `pi-agent-core` (agent runtime + tool calling), `pi-ai` (multi-provider
  streaming LLM client, 40+ providers), `pi-natives` (N-API: grep/shell/
  image/text/syntax), `pi-coding-agent` SDK (`SessionManager`,
  `createAgentSession`, typed events), `collab-web` (browser guest client),
  MCP + subagents + LSP-on-write + Ollama first-class (`ollama launch omp`).
- Two ideas worth stealing outright:
  - **Hash-anchored edits (hashline).** The model references content-hash
    anchors instead of retyping lines — whitespace battles disappear. This
    is *our* BLAKE3 content addressing wearing a different hat; our edit
    tool should speak hashes, not line numbers.
  - **Ollama as the local-model story.** No custom runner to build: Ollama
    is just another OpenAI-compatible endpoint.
- Reuse hazard: N-API natives don't load in a browser; Bun-isms in the harness.
  The TS packages (`pi-agent-core`, `pi-ai`) are the reusable part, mapped to
  our REST tools exactly like the opencode-SDK option — lighter than
  opencode, same CORS/key caveats in browser.

### Block/goose (`aaif-goose/goose`, Apache-2.0, ~54k stars) — Rust, but a product
- Genuinely Rust (workspace with `rust-toolchain.toml`), desktop app + CLI +
  embeddable API, 15+ providers, 70+ MCP extensions, ACP support
  (`test_acp_client.py` in-tree). Same weight class as the opencode sidecar:
  fine as an installable peer, wrong shape to absorb — we would fork a
  product, not gain a library.
- Takeaway: **ACP (Agent Client Protocol) is converging as the standard
  agent-embedding interface** (opencode, goose, omp-adjacent IDEs all speak
  it). Whatever we build should expose/consume ACP rather than inventing a
  fourth protocol.

### stippi/code-assistant, Forge, agentty — noted, not chosen
- Rust/C++ native agents with MCP and terminal UIs. Same verdict as goose:
  credible sidecars, no transport or storage advantage over what §3 proposes.

### Browser inference: wllama + WebLLM — the Pages answer
- **wllama** (`@localmode/wllama`): llama.cpp compiled to WASM, any GGUF from
  HuggingFace, works in *all* modern browsers (no WebGPU required),
  **OAI-compatible tool calling**, embeddings, vision variants, cached
  locally. **WebLLM** (MLC/TVM): faster via WebGPU where present,
  OpenAI-style API + JSON-mode function calling.
- Practical envelope: 1–3B models (Llama 3.2 1B, Qwen 2.5 1.5B) for triage,
  summarization, and repo Q&A fully offline on Pages; heavy codegen stays
  server-side. No API keys, no CORS, private by construction.

## 2. Key finding: our stack already contains an agent toolkit

- **Kernel syscall boundary** (`crates/os/src/api.rs`): open/read/write/
  seek/close/stat/unlink/readdir/mkdir/rename/du/df — the file-tool set,
  sandboxed, transport-agnostic.
- **WASM volume ops** (`os-wasm/src/os.rs`): `cat`/`write`/`touch`/`ls`/
  `stat` over localStorage — the same tools in the sandbox.
- **Code intel** (`parse_text`, tree-sitter + heuristic, `"engine"`-labeled)
  and **Tantivy search**: repo-map + context retrieval without new deps.
- **Secrets**: `keystore::{seal,open_sealed}` + `sync_secrets` side table —
  provider keys sealed at rest, merged at call time (already the rule for
  sync tokens).
- **Sessions want**: striped placement (`sync_files` manifests), version
  snapshots before writes (editor precedent), `lease_*` single-writer,
  `repair::tasks()` → `TaskTable` bridge (subagent rows show in `ps`).
- **Transports**: `invoke()` tri-transport dispatch, `REST_FIRST`,
  `isStaticHost()`/wasm bridge. Missing pieces, all small: chunked SSE in
  the hand-rolled server (polling fallback exists), an `fetch`-capable HTTP
  client in `os-wasm` (it already has `wasm-bindgen` +
  `wasm-bindgen-futures` + `serde_json`; web-sys fetch is additive).

## 3. Recommendation: custom Rust agent core (`crates/agent`)

A purpose-built loop is the only option that runs **natively on all three
transports** with **zero new runtimes**:

```
crates/agent  (no OS threads, no blocking I/O in core)
├── loop      turn(messages, tools) -> TurnOutcome  (caller drives I/O)
├── transport LLM dialect trait: Anthropic | OpenAI-compatible
│     native → reqwest (already a dep) · wasm → web-sys fetch · web → same
├── tools     trait with ONE schema set (OpenAI function format —
│             the same shape wllama and most providers accept)
│     native impl → Kernel + cybsh + tree-sitter + tantivy + editor writes
│     wasm impl   → volume ops + localStorage (+ dashboard proxy calls)
├── providers OpenAI-compatible first = OpenRouter + Ollama + wllama + most
│             gateways work day one; Anthropic dialect second
├── sessions  transcript → striped placement; versions before writes;
│             lease-scoped single writer; export/import JSON (opencode-compat)
└── protocols ACP endpoint (stdio + HTTP) so opencode/goose/omp IDEs can
              drive OUR volume, and our panel can drive THEIR servers
```

- **Edits** speak content hashes (hashline-style, verified against BLAKE3) —
  no line-number drift, idempotent replays, free dedup.
- **Permissions** reuse the opencode-compatible ruleset shape
  (`allow|ask|deny` + wildcards + `external_directory: deny` always), decided
  by our RBAC roles; `ask` surfaces through the existing permission UI.
- **Compute fan-out**: subagent `task` calls become provider-slot jobs;
  attaching a provider visibly parallelizes agent work (Tier-2 story intact).
- **Models**: cloud keys from keystore; local via Ollama endpoint (desktop/
  Docker, zero new code); offline Pages via wllama/WebLLM (small models,
  triage-grade; honest about the envelope).

## 4. How the alternatives slot in (not either/or)

| Need | Answer |
|---|---|
| Full-strength coding agent, desktop/Docker, now | opencode *or* omp *or* goose sidecar (`OPENCODE-PORT.md` pattern; omp's npm SDK is the lightest of the three) |
| Native agent on every transport, zero new runtime | **Custom `crates/agent` (this doc's recommendation)** |
| Offline Pages inference | wllama (all browsers) / WebLLM (WebGPU) as `crates/agent` endpoints |
| Local desktop models, no new code | Ollama OpenAI-compatible endpoint |
| IDE/protocol interop | ACP both directions; MCP server exposing kernel ops (lets any of the above reason over the merged volume) |
| 40+ providers on day one | OpenRouter (one key, OpenAI dialect) instead of N provider integrations |

## 5. Phased delivery

- **Phase 0 (core loop, desktop-usable):** `crates/agent` with OpenAI dialect
  over reqwest, Kernel tool impl, keystore keys, `cybsh ai` wiring, session
  save/load to striped placement. Definition of done: plan/build-style
  read-only + edit loop against the volume with `ask` approvals.
- **Phase 1 (all transports):** wasm tool impl + web-sys fetch transport,
  `AgentPanel.vue` on the tri-transport `invoke()` path, SSE-or-poll event
  tail, lease-scoped sessions, versioned saves.
- **Phase 2 (depth + interop):** Anthropic dialect, hash-anchored edit tool,
  subagent fan-out onto provider slots, ACP endpoint, MCP kernel server,
  wllama spike for offline Pages, `stats`/cost readout.

*Risks: provider dialect quirks (contained by OpenAI-first + a small
per-provider shim table); small-model tool-call reliability (mitigate with
hash-anchored edits + `ask` defaults + tight tool schemas); SSE needs
chunked streaming in our server (polling fallback already proven by
`sync status`); session blobs on metered providers (cap transcript sync to
`whole` placement, metadata only).*
