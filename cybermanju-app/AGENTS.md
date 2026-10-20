# AGENTS.md — CyberManju OS agent rules

> Source of truth for AI agents working in this repo. Verified against the
> tree on 2026-10-09 (v0.1.0, `com.cybermanju.os`). If a rule below conflicts
> with a doc, this file wins for workflow; `ARCHITECTURE.md` wins for design.

## 0. Project identity

- **Name:** CyberManju OS — quantum-resistant encrypted file manager + decentralized OS layer.
- **Version:** `0.1.0` — `package.json` is the single source of truth.
  Every other version carrier must agree (`bash scripts/check-version.sh`):
  `package-lock.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`,
  all 15 `crates/*/Cargo.toml`, `docker-compose.yml` (`x-casaos`), `aur/PKGBUILD`, `README.md`.
  Release tags must be `v*` and match (`check-version.sh --tag`).
- **Identifier:** `com.cybermanju.os` (Tauri + Flatpak `com.cybermanju.os.yml`).
- **License:** MIT (`LICENSE` must exist — CI asserts it).
- **Repo:** `cybermanju/cybermanju.github.io`, default branch `main`.
  GitHub Pages serves the WASM build; `main` pushes deploy automatically.

## 1. Stack (actual, not aspirational)

| Layer | Technology | Pinned fact |
|---|---|---|
| Desktop | Tauri v2 (`src-tauri/`, `productName` CyberManju OS, 1400×900 window) | 26 command modules in `src-tauri/src/commands/`, ~175 handlers wired in `lib.rs` |
| Backend | Rust 2021 workspace, 18 members (16 `crates/*` + `src-tauri` + `docker/server`) | resolver `2`; dev/test profiles use `line-tables-only`, no workspace deps for third-party |
| Frontend | Vue 3 Composition API + Pinia + TypeScript 5.8 + Vite 6.3 | 49 top-level `src/components/*.vue` (+26 `ui/` primitives), 34 `src/composables/*.ts`, 74 `src/utils/*.ts`, 1 Pinia store (`stores/app.ts`) |
| Reactive layer | `@vueuse/core` ^14.4.0 | Single shared hook `useSystemHardware()`; `useTitle`/`useBroadcastChannel`/`useDropZone` in use |
| Icons | `@iconify/vue` + `@iconify-json/solar` via `scripts/generate-icon-set.mjs` | `npm run typecheck` runs `icons` first — **unknown `solar:*` names fail the build** |
| Maps | `maplibre-gl` ^6.12.0 | GPS MapView with EXIF clustering |
| DB | `redb` (single file `cybermanju.db`, JSON rows) | 40 tables incl. `agent_sessions`, `agent_memories`, `disks/volumes/block_map`, `sync_*`, `leases`, `compute_tasks`, `schedules`, `secrets`, `oplog`, `devices`, `todos` |
| Search | Tantivy 0.22 BM25 + `crates/scene` heuristic | Scene: 17 categories × 10 langs, 740 keywords, weights name 1.0/tags 0.95/path 0.7 |
| Crypto | ChaCha20Poly1305 + `rustpq` (ML-KEM-1024 FIPS 203, ML-DSA-65 FIPS 204, SLH-DSA-128f FIPS 205) + Argon2id + BLAKE3 | Hybrid ML-KEM+X25519 supported |
| Compression | LZ4 → ZSTD-15 → Brotli-11 cascade | Per-layer stats, `.cyb3` payloads |
| Code intel | tree-sitter (rust/python/js/ts/go/bash) + heuristic fallback | Every result reports `"engine"`; heuristic strips qualifiers (`export const`, `pub mod`, Go receivers, bash defs) and hoists doc-comments into an additive `docs` field (`crates/types/src/code.rs`) |
| WASM | `crates/os-wasm` (agent/artifact/canal/compression/crypto/db/opfs/os) via `wasm-pack` + OPFS/localStorage | Pages transport = local volume + BM25-lite |
| Tests | Vitest (node env), 88 files / 925 tests in `tests/frontend/` | Includes `scene` (42), `memory`, `provider-canal/ship`, `hermes`, `agent-*`, `panel-aliases`, `spine-closeout`, `transport-parity-full`, `rest-hardening` |

### Crate map (`crates/`)

`agent` (loop/protocol/config/memory/providers/mcp/edit/redact/stream) ·
`os` (api/shell/task/compute) · `os-wasm` (browser volume) ·
`disk` (superblock/disk/allocator/catalog/volume) · `erasure` (RS k+m) ·
`web` (22 `api/*.rs` + `security.rs`) · `sync` (backends/pipeline/scrub/repair/gc/lease/state) ·
`db` · `types` · `crypto` · `compression` · `search` · `faces` · `scene` · `tests`.

### Frontend transports (`src/composables/useTauri.ts`)

`tauri` IPC (desktop) · `rest` (Docker/web `:3456`) · `wasm` (Pages).
`os_*/disk/*` are `REST_FIRST`. Settings shows the active transport
(`VITE_TRANSPORT` override). REST has 170 mapped commands in `REST_ROUTES` (`src/composables/useTauri.ts`).

## 2. Toolchain rules (hard)

- NEVER install or set up Rust/Cargo locally (`rustup`, `apt install cargo`,
  `cargo install`, etc.). No box linker exists by design.
  - No local `cargo fmt`, `cargo clippy`, or `cargo test` — rely on CI's
    **Rust Lint & Test** job for all Rust validation.
  - `ORT_CACHE_DIR=$HOME/.cache/ort` is required before any cargo command —
    another reason cargo stays in CI only.
- NEVER run `npm ci` from scratch unless CI parity demands it; prefer existing
  `node_modules`. (CI uses `npm ci` + `setup-node` npm cache.)
- Local verification loop (allowed, fast):
  `bash scripts/check-version.sh` → `bash scripts/check-file-headers.sh` →
  `npx vue-tsc --noEmit` → `npm test`
  (`npm run typecheck` also runs the icon generator).

## 3. Git / GitHub rules

- GH token is persisted in `~/.bashrc` (`GH_TOKEN`/`GITHUB_TOKEN`, before the
  interactive early-return) and `~/.config/gh/hosts.yml` (`gh auth login
  --with-token` with env cleared so credentials are stored, not env-bound).
  - Do NOT pass the token explicitly in every command; rely on stored auth
    (`gh auth status` must pass with no env override).
  - Old `gh` (2.4.0) has no `--branch` flag and weak `run view` status —
    use `gh api repos/.../actions/runs/...` + `/jobs` for CI state.
- Default branch is `main`. Push with `git push origin main`
  (no `-f` unless explicitly requested).
- When asked to push: `git add -A` + commit (descriptive message, not bare
  `update`) + push, then watch CI. `push.sh [-m msg] [--watch] ["message"]`
  does exactly this for both remotes (`origin` GitHub + `gitlab` mirror):
  heal `.git` ownership, stage all (aborts if `git add` fails), commit,
  fetch both remotes, auto-rebase onto `origin/main` when behind (never
  force-push), push main, push only tags the remote lacks (divergent remote
  tags are left alone with a warning).   `--watch`
  (or `PUSH_WATCH_CI=1`) chains into `scripts/watch-ci.sh --sha HEAD`,
  which polls the CI run via `gh api .../actions/runs/...` + `/jobs`
  and saves/prints the FULL logs of all steps (`logs/ci-<run-id>/`).
- Release via `push.sh --release -m "notes md"` (`--release-notes-file FILE`
  for long markdown): commits as `chore(release): vX.Y.Z`, pushes main, creates
  the `vX.Y.Z` tag and pushes it — the tag push triggers `release.yml` (full
  rebuild of every family); custom notes are prepended to the atlas body once
  published. `--release --last [--reuse-run ID]` instead dispatches
  `release.yml` in reuse mode: zero compilation, ships one green CI run's
  `dist-*` artifacts unchanged (auto-resolves HEAD's latest green run; pin with
  `--reuse-run` when HEAD is newer). `--move-tag` (full mode only) explicitly
  deletes/recreates an existing tag at HEAD and force-updates it on both
  remotes; refuses when a published GitHub release sits on the tag.
- CI skips **pushes that touch only prose** (`**.md`, `docs/**`, `LICENSE`).
  Code pushes run the full ~15 min pipeline; `pull_request` always runs all.

## 4. CI rules (`.github/workflows/ci.yml` + `release.yml`)

CI jobs (`ci.yml`, concurrency `CI-<ref>`, cancel-in-progress):
`meta-check` (versions + file headers + LICENSE) · `rust-check` (lock gate
`cargo metadata --locked` → fmt → clippy `-D warnings`
→ `cargo test --workspace --no-fail-fast`) · `docker-build` ·
`audit` (**blocking** since 2026-10-18 — 10 RustSec vulns fixed via the
quinn-proto 0.11.19 / rustls 0.23.46 / ruzstd 0.9.1 lock bump; GitLab mirror
drops `allow_failure` too) ·
`wasm-build` (wasm32 check → `wasm-pack` → `npm ci` → `vue-tsc` → `npm test`
→ `build:wasm:frontend` → `dist-wasm/`) · `windows-build` · `linux-build`
(deb+AppImage+rpm) · `rpm-build` + `flatpak-build` (from linux artifacts) ·
`arch-build` (Arch container, `NO_STRIP`/`APPIMAGE_EXTRACT_AND_RUN`) ·
`android-build` (NDK `28.2.13676358`, arm64-v8a release, APK+AAB `--split-per-abi`, signed; unsigned PR artifacts say `-unsigned`) ·
`macos-build` · `deploy-pages` (needs `wasm-build`, push-to-main only).

Release (`release.yml`, on `v*` tags + `workflow_dispatch`): same gates + Docker→GHCR +
`create-release` with all 8 artifact families present (windows/linux/rpm/
flatpak/arch/macos/android/wasm + SHA256SUMS). Dispatch mode takes
`tag`/`reuse_run_id`/`notes` inputs: with `reuse_run_id` every build job
(incl. `rust-check`) is skipped and `fetch-last-build` re-uploads that green
CI run's `dist-*` artifacts under `release-*` names, so `create-release` is
byte-identical in both modes. Release notes = generated
changelog + `docs/RELEASE_NOTES.md` (feature atlas + status, shared with GitLab).

GitLab mirror (`.gitlab-ci.yml`): same stages on SaaS Linux runners; Windows /
macOS jobs are `manual + allow_failure` (no SaaS runners — needs self-hosted
runners tagged `cybermanju-windows` / `cybermanju-macos`); tag releases ship
produced families + SHA256SUMS as Generic Packages with a GitLab Release.
Secrets mirror GitHub (`TAURI_SIGNING_PRIVATE_KEY`, `ANDROID_KEYSTORE_*`).

- Watch until all jobs green; fix and re-push on failure.
- No Dependabot in this repo (no `dependabot.yml`, no bot branches/PRs) —
  action pins in `ci.yml`/`release.yml` are bumped by hand (SHA + version
  comment), majors only after checking release notes against our inputs.
- Android signing: `scripts/android-signing.sh prepare|verify|cleanup`
  (+ `android-configure.sh` manifest/versionCode, `android-icons.sh`
  launcher icons, `android-smoke.sh` on-device best-effort); release requires `REQUIRE_SIGNING=1` + keystore
  secrets. Full runbook: `docs/ANDROID.md`.

## 5. Contracts agents must not break

- **Async jobs:** `POST /api/sync/start → 202 {jobId}`, poll
  `GET /api/sync/jobs/{jobId}`; agent runs identical
  (`POST /api/agent/prompt → 202`, approve/abort endpoints). Asks park ≤10 min, then auto-deny.
- **Error prefixes (AGENT-1):** `auth: / rate_limited: / not_found: /
  unsupported: (→501) / too_large: / integrity: / network:` + `disk_full:` +
  `conflict:` — mapped to UI hints, never raw.
- **Permissions à la opencode:** `allow|ask|deny` + wildcards, plan-agent
  read-only, `deny` beats auto-approve. Provider keys sealed
  (`agent:key:<config_id>`, `hasKey` only). MCP tools `mcp__server__tool`,
  same rules, admin-gated attach.
- **Hash-anchored edits (BLAKE3):** exact-once, whitespace-tolerant fallback,
  stale-anchor `integrity:` refusal, short-anchor (<16 hex chars / 64 bits)
  `integrity:` refusal, `conflict:` on ambiguity. Our trailer strips only
  when it verifies (round-trip hash, or pre-edit bytes on edit/write) —
  foreign anchor-shaped lines are content and stay.
- **Honest limits:** faces are engine-labeled (`onnx` / `heuristic-v2` /
  `none`) — ONNX silence stays silence, no-ONNX falls back to the labeled
  skin-segmentation heuristic, never fabricated; code parse
  reports `"engine"`; WASM `sync`
  needs the dashboard; `encrypted:/binary:/too_large:` refusals on content API
  (1 MiB cap, versioned saves).
- **Auth:** fail-closed, 404-before-401 on unknown routes, `Claims{role}`
  RBAC, bootstrap-only register, sealed secrets, JWT secret 0600.
  Agent transcripts are owner-bound (`owner_id`, admin-or-owner reads;
  legacy unowned rows stay readable); jobs were already owner-bound.
  Approval `remember` persists an allow-rule on admin REST calls only.
  The device shell spawns scrubbed (`env_clear`, PATH/locale only).
- **cybsh:** server-side shell via `POST /api/os/exec` (`--json`, pipes,
  history, completion); same shell on desktop/web/Pages (WASM dispatcher).
- **Memory:** transcripts in `agent_sessions` + semantic rows in
  `agent_memories` (2200-char recall budget); `memory_remember` goes through
  the permission gate; compaction hands off + auto-stores.
- **Failover:** `AgentConfig.fallbackIds` (max 4, validated on save) chains
  assistants as extra accounts/keys or providers; terminal `auth:/rate_limited:/
  network:/limit:` (incl. empty credits) continues the same transcript on the
  next route, others stop; all-failed errors append `(tried: …)`.

## 6. Docs map

- `ARCHITECTURE.md` — system diagram, module graph, data flows, DB schema,
  backends, compression, encryption, sync pipeline, ZimaOS, full REST table.
- `docs/OPERATIONS.md` — Docker/ZimaOS runbook, env table, disks/volume,
  sync+agent async contracts, `cybsh` reference.
- `docs/SECURITY.md` — threat model, no-TLS transport story, key/token
  management, hardening (AGENT-3).
- `docs/MEMORY.md` — two-layer memory (transcripts + semantic), Hermes
  mapping, budgets, contracts.
- `docs/HERMES-INSIGHTS.md`, `docs/AGENT-REVIEW.md` + `AI.md` (tracker),
  `docs/OPENCODE-PORT.md`, `docs/AI-AGENT-ALTERNATIVES.md`.
- `worklog.md` — dated change log (append entries; newest last).

## 7. Comment strategy (top-file headers, 100% enforced)

- Every code file (`.rs/.ts/.vue/.js/.mjs/.sh/.go`) carries a merged
  top-file header — file-level *what/why/who* in one stable block:
  `// CyberManju OS — <Module> (<path>).` + `Role:` + `Exports:` +
  `Integrates:` + `Contracts:`. Vue SFCs put it after the `<script>`
  opener; `.sh` uses `#`; `.vue` template-only files use `<!-- -->`.
- Triage (what merges up vs what stays): file-level narration merges UP
  into the header; item-level docs STAY inline at the item they guard —
  `///`/`/**`, `SAFETY:`, error prefixes (`auth:/integrity:/conflict:`…),
  budgets/caps, `engine` labels, `AGENT-*`/`P*` tags, section bars.
  Never hoist contracts away, never delete code to satisfy the gate.
- Grandfather: a leading `CyberManju` block with ≥2 lines of merged prose
  already passes; only missing / unnamed / single-line headers fail.
- Tooling (all cargo-free, local-safe):
  `scripts/check-file-headers.sh [--list|--fix]` (CI `meta-check` gate);
  `scripts/generate-file-headers.py --dry-run|--apply` (idempotent filler,
  header-only insertions at line 1 — audit with `git diff`, hunks must sit
  at line 1); `scripts/collect-file-context.mjs` (heuristic
  symbols/imports/comments JSON mirroring `cybermanju-types::code`);
  `scripts/check-symbol-docs.mjs [--all|--dir P|--json|--strict]` (advisory
  missing-item-docs auditor, public API by default — never a CI gate).
- Structure finder (`crates/types/src/code.rs::extract_symbols_heuristic`,
  the authoritative fallback for every transport): qualifier stripping
  (`export const`, `pub mod`, `pub(in…)`, `unsafe/async`, Go receiver
  parens), bash `name()` defs, top-level-only `const`/`static`, and
  doc-comment hoisting (`///`/`//!`/`/**`/`##`/`#:`/`"""` → additive
  `docs` field: consecutive lines join with a space, ≤400 chars, no
  blank/code line between docs and item — `#[…]`/`@…` attributes,
  `*/`/`-->` closers and plain narration are transparent and slide the
  window instead of consuming it).
  Plain `//` narration is never hoisted as docs text. Same JSON shape +
  `docs`; tree-sitter stays primary where grammars are bundled.
