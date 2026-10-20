<div align="center">

<img src="public/bhumisparsha.png" alt="Bhumisparsha School" width="120" />

# 🛡️ CyberManju OS

### *Your quantum-proof vault · Your private cloud · Your OS inside an OS*

> Quantum-resistant encrypted file manager with AI face grouping, triple-layer
> compression, code intelligence, GPS map view, web dashboard, and multi-user
> access control — plus a decentralized OS layer: sized `.cybermanju` disks
> merged into one volume, Reed–Solomon durability, and a real `cybsh` terminal.

[![CI](https://github.com/cybermanju/cybermanju.github.io/actions/workflows/ci.yml/badge.svg)](https://github.com/cybermanju/cybermanju.github.io/actions/workflows/ci.yml)
[![Release](https://github.com/cybermanju/cybermanju.github.io/actions/workflows/release.yml/badge.svg)](https://github.com/cybermanju/cybermanju.github.io/releases)
[![Pages](https://img.shields.io/badge/demo-GitHub_Pages-00D4FF?style=for-the-badge&logo=github)](https://cybermanju.github.io)
[![Tauri v2](https://img.shields.io/badge/desktop-Tauri_v2-FFC131?style=for-the-badge&logo=tauri)](https://tauri.app)
[![Vue 3](https://img.shields.io/badge/frontend-Vue_3-42b883?style=for-the-badge&logo=vue.js)](https://vuejs.org)
[![Rust](https://img.shields.io/badge/backend-Rust_2021-dea584?style=for-the-badge&logo=rust)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-00FF41?style=for-the-badge)](LICENSE)

**Version:** 0.1.1 \
**Identifier:** `com.cybermanju.os` \
**License:** MIT

[🚀 Quick start](#-60-second-launch) ·
[✨ Features](#-feature-atlas) ·
[🧠 AI agent](#-native-ai-agent-zero-new-runtimes) ·
[💽 OS layer](#-decentralized-os-cybsh--disks--durability) ·
[📡 API](#-api-control-tower) ·
[🗺️ Architecture](#️-how-it-fits-together)

</div>

---

## 🧭 Choose your adventure

| I want to… | Jump to | Time |
|---|---|---|
| 🏃 Run it **right now** | [60-second launch](#-60-second-launch) | 1 min |
| 🖥️ Install the **desktop app** | [Desktop builds](#️-desktop-app) | 5 min |
| 🐳 Host it on **NAS / Docker / ZimaOS** | [Self-host](#-docker--zimaos) | 5 min |
| 🌐 Try the **web demo** | [Pages / WASM](#-wasm--github-pages) | 0 min |
| 🤖 Talk to the **AI agent** | [Agent guide](#-native-ai-agent-zero-new-runtimes) | 3 min |
| 💽 Learn the **disk OS** | [cybsh + disks](#-decentralized-os-cybsh--disks--durability) | 5 min |
| 📚 Browse the **3D wiki** | [Interactive wiki](https://cybermanju.github.io/wiki/) (agents: [`llms.txt`](https://cybermanju.github.io/llms.txt)) | 2 min |
| 🛠️ Hack on the **code** | [Contributing & repo map](#-repo-map) | ∞ |

> [!TIP]
> **Three transports, one UI.** The frontend auto-detects where it runs:
> `tauri` IPC on desktop · `rest` on Docker/web (`:3456`) · `wasm` on Pages
> (local volume + BM25-lite). `os_*`/`disk_*` calls are `REST_FIRST` — the
> Settings panel always shows which transport is live (`VITE_TRANSPORT` can
> force one).

---

## ⚡ 60-second launch

```bash
curl -fsSL https://cybermanju.github.io/install.sh | bash
# installs `cyb`, then walks you through it: pick a backend
# (docker · native app · remote · CLI Limited) and run `cyb setup`
```

Windows PowerShell (native):

```powershell
irm https://cybermanju.github.io/install.ps1 | iex
# same walkthrough: backend menu, then `cyb setup`
```

Open `http://localhost:3456` from any device on your LAN — same vault, new screen. ✨
Checksum-verified against the release `SHA256SUMS`; pin a release with `CYB_VERSION=v0.1.1`.

**Android / Termux:** run the same command in Termux to install the Go CLI only (not the Android app). ARM64 is the supported Termux target. It updates an existing writable `cyb` on `PATH`, otherwise installs to `$PREFIX/bin`; re-running replaces it with the latest CLI. `cyb setup` first asks whether to connect to a Remote Dashboard (full CLI) or use CLI Limited (Supabase direct, read-only), then requests only the details for that setup type. No root access is needed; install missing prerequisites with `pkg install curl tar coreutils`. The Android tarball starts shipping with the next tagged release; `v0.1.1` predates this support and has no Android asset.

<details>
<summary>⚙️ Preselect / automate the installer (click to expand)</summary>

Env goes **before `bash`** (stdin already carries the script), or use flags
with `bash -s --`:

```bash
# straight to a local dashboard, no menu
curl -fsSL https://cybermanju.github.io/install.sh | CYB_BACKEND=docker bash
# install only, configure later by hand
curl -fsSL https://cybermanju.github.io/install.sh | CYB_SETUP=0 bash
# fully unattended: docker backend + account in one shot
curl -fsSL https://cybermanju.github.io/install.sh | CYB_SETUP=1 CYB_BACKEND=docker CYB_USERNAME=you CYB_PASSWORD=secret bash
# same via flags
curl -fsSL https://cybermanju.github.io/install.sh | bash -s -- --backend docker --setup
```

| Knob | Values | Purpose |
|---|---|---|
| `CYB_SETUP` | `1` / `0` / `auto` (default) | run / skip / offer `cyb setup` |
| `CYB_BACKEND` | `docker` · `native` · `remote` · `supabase` (CLI Limited) | `serve up` / app dashboard / URL / read-only broker |
| `CYB_SERVE` | `1` | shorthand for `CYB_BACKEND=docker` |
| `CYB_SERVER` | URL | dashboard for `remote` + `setup` |
| `CYB_PORT` / `CYB_DATA` / `CYB_IMAGE` | port / dir / image | `serve up` + `native` customization |
| `CYB_USERNAME` / `CYB_PASSWORD` / `CYB_DISPLAY` | strings | unattended `cyb setup` |
| `CYB_SUPA_URL` / `CYB_SUPA_KEY` | URL / key | `supabase` broker pair |
| `CYB_YES` | `1` | assume yes to prompts |
| `CYB_VERSION` / `INSTALL_DIR` | tag / dir | pin release · install location |

PowerShell takes the same knobs as `$env:` variables
(`$env:CYB_BACKEND='docker'`, `$env:CYB_SETUP='0'`, …).
No TTY (CI/cron) means install-only unless `CYB_SETUP=1` or flags force it.
Full reference: `install.sh --help` (header comment lists everything).

</details>

<details>
<summary>🛠️ Prefer building from source? (click to expand)</summary>

```bash
# 1 — clone
git clone https://github.com/cybermanju/cybermanju.github.io.git
cd cybermanju.github.io

# 2 — frontend deps (Node 24+)
npm install

# 3 — desktop + HMR (web dashboard rides along on :3456)
npm run tauri:dev
```

</details>

<details>
<summary>📦 Prerequisites per platform (click to expand)</summary>

| Platform | What you need |
|---|---|
| **Linux Debian/Ubuntu** | `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev` |
| **Arch / CachyOS** | `webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg libsoup3 pkg-config base-devel openssl nodejs npm` |
| **macOS** | Xcode Command Line Tools — nothing else |
| **Windows** | WebView2 Runtime (pre-installed on Win 10/11) |
| **Android** | NDK `28.2.13676358`, JDK 17, `compileSdk/targetSdk 36` (CI-pinned) |

Rust **1.85+** via [rustup](https://rustup.rs/) for every native build.
No Rust needed for the web demo.

</details>

---

## 🌟 Feature atlas

```
  🔐 PQC vault ──┐
  🗜️ cyb3 pack ──┤
  🔎 BM25 find ──┼──►  📁 ONE VAULT  ──►  🗺️ map  📸 faces  </> code  🤖 agent
  🛰️ sync ───────┤          │
  💽 disks ──────┘          └──►  🖥️ desktop · 🐳 docker · 🌐 pages
```

### 🔐 Post-quantum vault

- **NIST FIPS 203** — ML-KEM-1024 key encapsulation · **FIPS 204** — ML-DSA-65 signatures · **FIPS 205** — SLH-DSA-128f hash signatures
- **Hybrid mode** ML-KEM + X25519 for transitional defense-in-depth
- **ChaCha20Poly1305** AEAD (96-bit CSPRNG nonces) + **BLAKE3** integrity on every cycle + **Argon2id** password hashing

### 🗜️ Triple-layer compression

| Layer | Engine | Vibe |
|---|---|---|
| 1 ⚡ | **LZ4** | ~400 MB/s — instant previews |
| 2 ⚖️ | **Zstandard-15** | balanced ratio / speed |
| 3 🗿 | **Brotli-11** | archival maximum |

Cascade `LZ4 → ZSTD → Brotli` (or any single layer) with per-layer stats in every `.cyb3` payload.

### 🔎 Search that finds things

Tantivy **BM25** over filename + content + tags — terms, phrases, booleans,
wildcards, fuzzy — plus faceted filters (type · encryption · GPS) and real
autocomplete from the term dictionary.

> [!NOTE]
> **Honest scenes, not fake vision.** `crates/scene` scores **17 categories ×
> 10 languages (740 keywords)** from filename / path / your tags only
> (weights `1.0 / 0.7 / 0.95`). Every badge shows its receipts:
> `BEACH 87% — matched praia`. The TypeScript twin parses the same JSON, so
> desktop, server and WASM always agree.

### 📸 Faces · 🗺️ Map · </> Code

- **Faces** — 128-dim embeddings, cosine distance, Union-Find clustering into
  person groups. Without the ONNX model the pipeline returns **empty, never
  fabricated**.
- **Map** — EXIF GPS extraction → MapLibre GL markers + clustering.
- **Code** — real tree-sitter for Rust/Python/JS/TS/Go/Bash, heuristic fallback
  otherwise; every parse reports `"engine"`. Includes a VS-like tabbed editor
  (<kbd>Ctrl</kbd>+<kbd>E</kbd>) with outline, find, dirty tracking and
  version-snapshotted saves.

### 👥 People & sharing

Role-based access (`admin` / `user` / `viewer`), per-file `read` / `write` /
`admin` grants, JWT sessions, collections, loose groups, style tags.

### 🛰️ Sync backends

`Local` · `GitHub` (Contents + Releases to 2 GB) · `Google Drive` (v3) ·
`GitLab` — pipeline `compress → preview → upload → link → clean` with
live ETA + cancel. Runs are **async by design**:

```bash
POST /api/sync/start   →  202 { jobId }
GET  /api/sync/jobs/{jobId}   # poll turns / progress / errors
```

Errors speak a contract, never raw text:
`auth:` · `rate_limited:` · `not_found:` · `unsupported:` (→501) ·
`too_large:` · `integrity:` · `network:` · `disk_full:` · `conflict:`

---

## 🤖 Native AI agent — zero new runtimes

Pure-Rust core (`crates/agent`) runs on **desktop, server and WASM** — tools
execute against the Kernel/volume, never a sidecar.

<details open>
<summary><b>🧩 Providers, tools & permissions</b></summary>

- **10 presets + custom** — Anthropic · OpenAI · OpenRouter · Ollama · Gemini ·
  Groq · Mistral · DeepSeek · xAI · Cerebras — per-config endpoint, model,
  dialect (OpenAI/Anthropic), auth scheme. Keys are **sealed**
  (`agent:key:<config_id>`, UI sees `hasKey` only).
- **Permissions à la opencode** — `allow | ask | deny` + wildcards,
  read-only plan-agents, `deny` always wins. Asks park as 202-jobs (≤10 min,
  then auto-deny) with UI approval cards.
- **Tools with a metaprompt** — `read / write / edit / list / grep (regex) /
  glob (**​**) / bash / task / question`, each documented for the model
  (read-before-edit, verify-after-act).
- **MCP servers** per config (stdio + Streamable HTTP, admin-gated) surface as
  `mcp__server__tool` under the same rules. Sessions compact into fresh ones
  (old kept), export/import like files.
- **Hardened loop** — rate-limit backoff, doom-loop guard, truncation notes,
  secret redaction, repo rules (`AGENTS.md` / `SKILL.md`), `cybsh ai` surface,
  offline-capable Pages loop via local gateways.

```bash
POST /api/agent/prompt  →  202 { jobId }   # detached run
GET  /api/agent/jobs/{id}                   # turns · usage · approvals
POST /api/agent/jobs/{id}/approve           # answer ask/question
POST /api/agent/jobs/{id}/abort             # cancel
```

</details>

<details>
<summary><b>🧠 Memory: transcripts + meaning</b></summary>

Two layers, one contract (`docs/MEMORY.md`): **verbatim transcripts**
(`agent_sessions`) plus **semantic memory** (`agent_memories`, 2200-char
recall budget). `memory_remember` passes the permission gate; compaction
hands off *and* auto-stores. Hermes-pattern compatible export included.

</details>

---

## 💽 Decentralized OS — cybsh · disks · durability

```
providers ─► .cybermanju disks (CYBMJU1 sealed superblock)
   ─► merged volume (spanned placement, df grows on attach)
   ─► durability (scrub → repair · RS k+m · rebuild-from-remote · GC · leases)
   ─► cybsh ─► Kernel ─► REST / Tauri-REST_FIRST / WASM
```

```bash
# cybsh — the same shell on desktop, web & Pages (Terminal panel or Ctrl+`)
disk create <config> 512M   # sized virtual disk per provider
disk attach <disk-id>        # volume grows — `df` proves it
disk resize <disk-id> 10G
 df · du · ls · cat · cp · mv · rm · stat · grep · find · head · tail · wc · write · edit · sync · oauth · compress · decompress · scrub · repair · gc · lease · ps · top · jobs · compute · keygen · search · run · theme · ui
 ls --json   # every command speaks JSON too — pipes, &&, ||, ;, history, completion
 ```

- **Admission before upload** — `disk full:` refuses before any byte lands. No partial objects.
- **Portable blocks** — `GET/PUT /api/volume/block/{lba}` (base64 + ranges) works
  everywhere; FUSE mount is Linux-desktop only.
- **Durability** — BLAKE3 scrub → replica/RS repair queue,
  `rebuild-from-remote` (survive losing the laptop), refcount GC (never deletes
  a referenced chunk), single-writer leases (`conflict:` on contention).

> [!WARNING]
> **Honest limits, on purpose.** WASM `sync` needs the dashboard. The content API caps at 1 MiB with
> versioned saves and `encrypted:` / `binary:` / `too_large:` refusals.
> Auth is fail-closed (404-before-401, RBAC `Claims{role}`, bootstrap-only
> register, 0600 secrets).

---

## 🧱 Tech stack

| Layer | Technology |
|---|---|
| 🖥️ Desktop | Tauri v2 (`com.cybermanju.os`, 1400×900) — 26 command modules, ~175 handlers |
| 🦀 Backend | Rust 2021 workspace — **16 crates** + `src-tauri` + `docker/server` |
| 🎨 Frontend | Vue 3 Composition + Pinia + TS 5.8 + Vite 6.3 — **49 components · 34 composables · 74 utils** |
| ✨ Reactive | `@vueuse/core` 14 — one shared `useSystemHardware()` for the whole OS |
| 🎨 Icons | `@iconify/vue` Solar set (generated — unknown `solar:*` **fails the build**) |
| 🗺️ Maps | MapLibre GL 6 |
| 🗄️ DB | redb single-file `cybermanju.db` (JSON rows, 40 tables) |
| 🔎 Search | Tantivy 0.22 BM25 + scene heuristic |
| 🔐 Crypto | ChaCha20Poly1305 · ML-KEM-1024 · ML-DSA-65 · SLH-DSA-128f · Argon2id · BLAKE3 |
| 🗜️ Pack | LZ4 → ZSTD-15 → Brotli-11 |
| </> Parse | tree-sitter (6 real grammars) + heuristic fallback |
| 🌐 WASM | `crates/os-wasm` via `wasm-pack` + OPFS — full OS in the browser |
| ✅ Tests | Vitest **88 files / 925 tests** + Rust workspace tests in CI |

---

## 📡 API control tower

<details>
<summary><b>🔌 Tauri IPC (desktop) — 35 commands</b></summary>

Registered in `src-tauri/src/lib.rs` across `files · accounts · collections ·
compression · encryption · faces · map · search · sync · users · dashboard ·
import · agent · disk · share · trash · versions · batch · audit`. Full data
flows in [`ARCHITECTURE.md`](ARCHITECTURE.md).

</details>

<details>
<summary><b>🌍 REST (Docker/web :3456) — 135+ routes</b></summary>

| Area | Highlights |
|---|---|
| Files | `GET /api/files` · `GET/DELETE /api/files/{id}` · `GET/PUT /api/files/{id}/content` (1 MiB, versioned) · `PUT /api/files/{id}/tags` |
| Library | `/api/accounts` · `/api/collections` · `/api/collection-items` · `/api/face-groups` · `/api/loose-groups` · `/api/geo-files` |
| Crypto | `/api/encryption/status` · `/api/encryption/keys` |
| Search | `/api/search?q=` · `/api/locations` |
| Users | `/api/users` · `POST /api/users/register` · `POST /api/users/login` · `/api/permissions…` |
| Agent | `/api/agent/providers` · `/api/agent/configs…` (+`/key`, `/models`, `/mcp…`) · `/api/agent/sessions…` · `/api/agent/prompt` · `/api/agent/jobs…` |
| OS | `POST /api/os/exec` · `/api/os/{complete,stat,ls,du,df,ps,top,workers,jobs}` · `/api/disk/*` · `/api/volume/block/{lba}` |
| Ops | `/api/sync/*` · `/api/repair/*` · `/api/scrub/*` · `/api/lease/*` · `POST /api/code/parse` · `GET /api/health` · `GET /api/readyz` |

Auth: `Authorization: Bearer <token>` · fail-closed · 404-before-401.

</details>

---

## 🏗️ Build & ship

### 🖥️ Desktop app

```bash
npm run typecheck        # icons + vue-tsc (must stay green)
npm run tauri:build      # → target/release/bundle/
npm run tauri:build:debug  # faster, larger
```

> Arch/CachyOS needs `NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1` (see CI) —
> linuxdeploy can't read Arch's `.relr.dyn` sections or mount FUSE in containers.

Install from source or via AUR (`aur/PKGBUILD`): `cd aur/ && makepkg -si`.

### 🐳 Docker & ZimaOS

```bash
docker build -t cybermanju-os:latest .
docker compose up -d   # → http://<nas-ip>:3456
```

Hardened compose: `read_only` · `cap_drop: ALL` · `no-new-privileges` ·
`mem_limit: 1g` · LAN-only port · healthcheck on `/api/readyz`.
ZimaOS metadata (`x-casaos`, amd64+arm64) is baked into `docker-compose.yml`.

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | `3456` | dashboard port |
| `DB_PATH` | `/data/cybermanju.db` | redb file (single handle) |
| `STATIC_DIR` | `/app/static` | built frontend |
| `RUST_LOG` / `TZ` | `info` / `UTC` | logs / timezone |
| `CYBERMANJU_JWT_SECRET` | random/file | 0600 persisted secret |
| `CYBERMANJU_MASTER_PASSPHRASE` | unset | keystore + superblock seal |
| `ORT_CACHE_DIR` | `$HOME/.cache/ort` | **required** before any cargo cmd |

Full runbook → [`docs/OPERATIONS.md`](docs/OPERATIONS.md) ·
threat model → [`docs/SECURITY.md`](docs/SECURITY.md).

### ⌨️ Universal CLI (`cyb`)

Paste-ready install — checksum-verified against the release `SHA256SUMS`.
The binary lands in `~/.local/bin` (`%LOCALAPPDATA%\cybermanju\bin` on
Windows); pin a release with `CYB_VERSION=v0.1.1` (`$env:CYB_VERSION`).

Linux / macOS (also Git-Bash/MSYS2 on Windows):

```bash
curl -fsSL https://cybermanju.github.io/install.sh | bash
```

Windows PowerShell (native):

```powershell
irm https://cybermanju.github.io/install.ps1 | iex
```

```bash
cyb setup                 # server → account → saved profile (0600)
cyb serve up              # dashboard via Docker (or point at one: cyb config set-server URL)
cyb oauth launch google --config <id>   # browser approval, verified after
cyb disk create <config> 10G            # encrypted vault disk (passphrase-sealed)
cyb sync move <file> <from> <to>        # single-copy move between providers
cyb sh "disk list" | cyb run job.cybsh | cyb repl   # the same cybsh everywhere
cyb ai prompt <config> "summarize the vault" --follow
cyb files cat <id> -r      # glamour-render markdown to the terminal
```

Every screen is Lipgloss neon (hot pink → signal red, cyan keycaps,
zebra tables, tinted `--json`); agent answers and `cat -r` render
markdown via Glamour (`GLAMOUR_STYLE` wins when set). Go
(cobra + Charm v2: Bubble Tea, Lipgloss, Bubbles, Huh, Glamour) in `cli/`, checksum-verified
self-updates (`cyb update`). Releases ship seven `cyb-<os>-<arch>.tar.gz`
archives (six desktop targets plus Android/Termux arm64),
`cyb_<ver>_<arch>.deb` × 2, plus `SHA256SUMS` alongside the desktop
families (`install.sh` uses tarballs; Windows zips carry `cyb.exe`).

Backends (`cyb backend use …`) — one CLI, four ways to reach a vault:
`remote` (any dashboard URL) · `docker` (`cyb serve up` under the hood) ·
`native` (installed deb/AppImage/dmg/apk/Windows app's localhost
dashboard, auto-probed) · `CLI Limited` (`supabase`, broker-direct, read-only: Supabase
Auth + provider repo/file/project lists, encrypted blobs stay opaque —
full power needs a dashboard). `cyb doctor` checks all of them.

| Area | Commands |
|---|---|
| Onboarding | `setup` (splash → probe → register/login) · `serve up/status/stop` (Docker) · `login/logout` · `config` · `doctor` (8 health checks) |
| Files | `files ls/info/mkdir/rename/trash/restore/empty-trash` · `cat/write` (1 MiB, versioned) · `versions/snapshot/revert/snapshot-all` · `batch delete/encrypt/compress` |
| Sync | `sync configs/create/rm/start --wait/job/status/progress/cancel` · `move` (provider↔provider) · `test/remote/usage/runs/restore/remote-rm/create-repo/upload` |
| Disks | `disk list/create/attach/detach/resize/check/df/key-holder/destroy` (Argon2id+ChaCha, passphrase never logged) |
| OAuth | `oauth launch <github\|google\|gitlab> --config` (browser + verify) · `oauth status` |
| Organize | `collections` · `accounts` · `loose` · `faces` (engine-labeled) · `share` · `audit` |
| Ops | `sh` (one line) · `run` (.cybsh --dry/--lint/--fmt) · `repl` (Bubble Tea) · `cron` · `repair/scrub/lease` · `search/suggest/geo` · `secrets` (sealed) · `crypto` · `code parse` · `users/perms` (admin) |
| AI | `ai providers/configs/new/session/forget/import` · `prompt --follow` (Glamour) · `status/abort/approve` · `init/compact/key/models/mcp*/mem` |

### 📱·🍎·🪟 Everywhere else

CI builds **Windows** (MSI/NSIS) · **Linux** (deb + AppImage + rpm) · **RPM** ·
**Flatpak** (`com.cybermanju.os`) · **Arch** · **macOS** (dmg) · **Android**
(signed arm64 APK, NDK `28.2.13676358`) · **CLI** (`cyb` × linux/darwin/windows
amd64+arm64 plus Android/Termux arm64) — releases ship all 9 families + `SHA256SUMS`. Tags `v*` trigger
`release.yml` (Docker → GHCR included).

#### Linux graphics checks (AppImage / .deb on Wayland, X11, VMs)

The desktop shell is GPU-composited by WebKitGTK, so a driver or compositor
quirk shows up as scrolling/animation jank, a black window, or paint
tearing — never as an error. These are **diagnostic** one-shot overrides
(the app never sets them for you); run each as an A/B test:

```bash
# Mesa/AMD/Intel: WebKit's DMABUF renderer is the usual suspect for
# black/garbled or stalling surfaces — fall back to the plain renderer.
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./CyberManju*.AppImage

# Suspect the driver entirely? Force llvmpipe (software GL).
# Smooth here but not on the GPU path => mesa/driver is the culprit.
LIBGL_ALWAYS_SOFTWARE=1 ./CyberManju*.AppImage

# WebKit compositing off — helps in some VM / remote / nested sessions.
WEBKIT_DISABLE_COMPOSITING_MODE=1 ./CyberManju*.AppImage

# Wayland session where WebKitGTK's Wayland path misbehaves: run on X11.
GDK_BACKEND=x11 ./CyberManju*.AppImage
```

Still janky under `LIBGL_ALWAYS_SOFTWARE=1`? Then it is not the GPU — the
remaining levers are built into the app itself: every Tauri command runs
off the main thread (`#[tauri::command(async)]`), polling pauses while the
window is hidden, drag/resize writes are rAF-coalesced, and the heavy
panels are code-split out of the boot bundle.

### 🌐 WASM / GitHub Pages

```bash
npm run build:wasm:frontend   # → dist-wasm/ (Pages deploys on every main push)
```

---

## 🗂️ Repo map

```
├── src/ (49 components · 34 composables · 67 utils · App.vue · main.ts)
│   ├── components/  FileManager · DesktopShell · AgentPanel · DevicesPanel · TerminalPanel · CodeStudio · ArchivePanel · ShieldPanel · SearchPanel …
│   ├── composables/ useTauri (3 transports) · useSystemHardware · useAgent · useProviderCanal …
│   ├── stores/app.ts · types/ · utils/ · workers/ · keymaps.ts
├── crates/ (16)  agent · os · os-wasm · disk · erasure · web (22 api) · sync · db
│                 types · crypto · compression · search · faces · scene · tests · launcher
├── src-tauri/ (26 command modules · lib.rs · tauri.conf.json 1400×900)
├── cli/ (Go `cyb`: cobra + lipgloss/bubbles/huh/glamour · REST-first)
├── install.sh + install.ps1 (checksum-verified universal installer)
├── docker/server/ (headless web entrypoint) · Dockerfile · docker-compose.yml
├── tests/frontend/ (25 files · 281 tests — scene 42 · memory · canal/ship · hermes · panel-aliases)
├── scripts/ (check-version · android-signing · os-* ops · generate-icon-set)
├── docs/ OPERATIONS · SECURITY · MEMORY · HERMES-INSIGHTS · AGENT-REVIEW · OPENCODE-PORT …
├── .github/workflows/ ci.yml (14 jobs) · release.yml (v* → 9 families + GHCR)
├── .gitlab-ci.yml (GitLab mirror: SaaS-Linux jobs run; windows/macos are
│   manual + allow_failure until tagged runners exist; release ships produced
│   families + SHA256SUMS as Generic Packages with a GitLab Release)
├── ARCHITECTURE.md · AI.md (agent tracker) · AGENTS.md (agent rules) · worklog.md
└── aur/PKGBUILD · index.html · vite.config(.wasm).ts · vitest.config.ts
```

---

## 🤝 Contributing

```bash
bash scripts/check-version.sh   # versions agree everywhere (package.json rules)
npx vue-tsc --noEmit             # icons + types
npm test                         # 25 files / 281 tests
```

> [!IMPORTANT]
> **Rust lives in CI.** Never install/run Cargo locally — no linker on dev
> boxes by design. Push code and let the `Rust Lint & Test` job run
> `fmt --check` → `clippy -D warnings` → `cargo test --workspace`.
> Prose-only pushes (`**.md`, `docs/**`, `LICENSE`) skip CI automatically;
> everything else runs the full ~15 min pipeline (concurrency cancels stale runs).

---

## 📚 Library

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — diagrams, module graph, DB schema, REST table
- [`docs/OPERATIONS.md`](docs/OPERATIONS.md) — runbook, env, disks, jobs, cybsh
- [`docs/SECURITY.md`](docs/SECURITY.md) — threat model, transport, hardening
- [`docs/MEMORY.md`](docs/MEMORY.md) — agent memory + Hermes mapping
- [`docs/AGENT-REVIEW.md`](docs/AGENT-REVIEW.md) + [`AI.md`](AI.md) — review + tracker
- [`docs/OPENCODE-PORT.md`](docs/OPENCODE-PORT.md) · [`docs/AI-AGENT-ALTERNATIVES.md`](docs/AI-AGENT-ALTERNATIVES.md)
- [`AGENTS.md`](AGENTS.md) — rules for AI agents · [`worklog.md`](worklog.md) — changelog

---

<div align="center">

### 💜 Built for people who own their data

*Encrypt everything. Sync anywhere. Lose nothing.*

**MIT** · by the CyberManju team · `cybermanju.github.io`

</div>
