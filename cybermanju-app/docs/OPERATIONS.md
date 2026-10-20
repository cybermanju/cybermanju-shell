# CyberManju OS — Operations Runbook

How to run the decentralized OS in production: Docker/ZimaOS, backup, env, TLS, health, and the `cybsh` terminal.

## 1. Deploy

```bash
docker compose up -d
# dashboard: http://<nas-ip>:3456
```

Hardened compose: `read_only`, `cap_drop: [ALL]`, `no-new-privileges`, `mem_limit`, LAN-only `3456`, healthcheck `GET /api/health` (readiness: `GET /api/readyz`).

## 2. Environment

| Variable | Default | Notes |
|---|---|---|
| `PORT` | `3456` | Dashboard port |
| `DB_PATH` | `/data/cybermanju.db` | redb file (exclusive lock — one handle; app + dashboard share it) |
| `STATIC_DIR` | `/app/static` | Built Vue frontend |
| `RUST_LOG` | `info` | |
| `TZ` | `UTC` | |
| `CYBERMANJU_JWT_SECRET` | random/file | Persisted 0600 at `<data>/jwt_secret`; rotation logs out sessions |
| `CYBERMANJU_ADMIN_USERNAME/_PASSWORD` | unset | Headless bootstrap admin (bootstrap-only register otherwise) |
| `CYBERMANJU_MASTER_PASSPHRASE` | unset | Keystore + disk superblock seal fallback |
| `CYBERMANJU_DATA_DIR` | app dir | Disks, keystore, oauth side-store |
| `CYBERMANJU_OAUTH_REDIRECT_URI` | `http://127.0.0.1:{port}/callback` | Desktop loopback; server uses redirect |
| `CYBERMANJU_GITLAB_INSTANCE_URL` | `https://gitlab.com` | Self-hosted override |
| `ORT_CACHE_DIR` | `$HOME/.cache/ort` | **Required** before any cargo command (ort-sys panics without it) |

## 3. Disks & volume (the OS storage)

Each provider holds a sized `.cybermanju` disk (`CYBMJU1` sealed superblock). The volume merges all attached disks; `df` grows on attach.

```bash
# cybsh (Terminal panel, or POST /api/os/exec {"line":"…"})
disk create <config-id> 512M
disk attach <disk-id>
disk resize <disk-id> 10G
disk check <disk-id>
df
```

- Spanned placement fills disk A to its high-water mark before B; round-robin available as policy.
- Admission control refuses with `disk full:` **before** any upload — no partial objects.
- `PUT /api/volume/block/{lba}` (base64) / `GET` with `{"start","end"}` range is the portable block path (Termux/Docker/web). FUSE (`cybermanju mount`) is Linux-desktop only with `/dev/fuse` probe.

## 4. Sync runs (async by design)

`POST /api/sync/start → 202 {jobId}` (never blocks the 5s request thread), poll `GET /api/sync/jobs/{jobId}` or `GET /api/sync/progress`, `POST /api/sync/cancel {jobId?}`. `cybsh sync start [config] [files…]` runs the same detached job via `POST /api/os/exec` (intercepted lock-free, before the request lock); bare-library `execute()` without a worker still answers `unsupported:` honestly.

Error prefixes (AGENT-1 contract) → UI hints (`describeSyncError`): `auth: / rate_limited: / not_found: / unsupported: (→501) / too_large: / integrity: / network:`, plus `disk_full:` and `conflict:`.

Encrypt-before-upload (`compress → encrypt CYBE1`, keystore handle in `sync_files`) is default-on; striped placement (`whole|striped`, `parity:1` = one replica, RS `k+m` under the hood) needs ≥2 enabled configs.

Unified-disk placement: single-copy home is the default — a file lives on ONE provider (`sync_files.home_config_id`); `SyncConfig.mirror=true` opts a provider back into duplicate-everywhere. `POST /api/sync/move {fileId, fromConfigId, toConfigId}` (Tauri `move_sync_file`, `cybsh sync move <file> <from> <to>`, Sync panel Move form) relocates verified — download A → upload B → BLAKE3 verify → delete A → retarget record; `mv /providers/<a>/x /providers/<b>/y` moves namespace bytes the same way. Vault/secret paths (`*.cybermanju`, `cybermanju-up-*.cyb3`, `master.passphrase`, `keystore.json`) are never synced or moved. One-folder setup keeps `<picked>/vault.cybermanju` beside `<picked>/files/` (the `local` root). Key holder: one provider/disk unwraps the others (`SyncConfig.keyHolder`, `DiskRow.holdsKeys`, `POST /api/disk/key-holder`, single-holder invariant enforced server-side).

## 4a. Provider namespace (`/providers/…` in cybsh)

Outside-container files — every Google Drive folder and GitHub/GitLab repo
behind a connected sync config — are addressable from the terminal as
`/providers/<mount|config>[/path]` (`<head>` is a VFS mount id or a sync
config id directly, so it works before any mount exists; `ls /providers`
lists mounts plus enabled-but-unmounted configs). Same namespace on desktop,
Docker, and Pages (the static shell serves it through the provider canal).

```bash
ls /                                 # vault volume + providers/ (whole virtual drive)
ls /providers                          # every mount/config
ls /providers/<id>/docs                # files + folders, folders first
cat /providers/<id>/README.md          # 1 MiB print cap (cp to read fully)
cp /providers/<a>/f.md /notes.md       # provider → vault
cp /notes.md /providers/<b>/f.md       # vault → provider (verified)
mv /providers/<a>/f /providers/<b>/g   # copy → BLAKE3 verify → delete source
rm /providers/<id>/old.md / rm -r /providers/<id>/dir
mkdir -p /providers/<id>/new/dir       # `.keep` marker (git has no empty dirs)
stat /providers/<id>/f.md [--json]
cd /providers/<id>/docs                # relative paths keep working after
```

`ls /` always shows the whole virtual drive: the vault volume plus a
synthetic `providers/` entry for this namespace (a real volume directory
literally named `providers` wins, and this namespace shadows it).

`du|find|grep|head|tail|wc|write|edit|encrypt|decrypt|compress|decompress`
stay volume-only and refuse provider operands with `unsupported:` (cp the
file locally first). `local`-backend configs refuse too (a host path, not a
remote). Drive listings merge files + an explicit folder query (its file
listing hides folders); git listings synthesize one level from the
recursive tree. A volume directory literally named `providers` is shadowed by
this namespace.

## 4a2. Host filesystem (`-os` in cybsh, native app only)

The same file verbs transpose onto the host (device) filesystem with `-os`:
the user's home directory on desktop (`$HOME` / `%USERPROFILE%`), shared
storage on Android (`/sdcard`). Both operands of `cp -os`/`mv -os` are host paths; in `-os` mode
even a literal `/providers/…` is a host path.

```bash
ls -os                                # home dir on desktop (sdcard on Android)
ls -os /sdcard/Download               # absolute or host-relative paths
cat -os /sdcard/a.txt                 # 1 MiB print cap (cp it to read fully)
cp -os /sdcard/a.txt /vault-drop/a.txt
mv -os /sdcard/old.txt /sdcard/new.txt  # cross-filesystem = copy → verify → delete
rm -os /sdcard/tmp.txt                # rm -os -r for host dirs
mkdir -os -p /sdcard/new/dir
touch -os /sdcard/new/note.txt
stat -os /sdcard/a.txt [--json]
du -os /sdcard/Download [--json]
cd -os /sdcard/Music                  # host cwd (volume cwd untouched)
pwd -os
```

Host verbs are `ls/cd/pwd/cat/cp/mv/rm/mkdir/touch/stat/du/df` plus the
read verbs `find/grep/head/tail/wc` and `write` — any other verb
with `-os` refuses with `unsupported:` instead of touching the volume, and
provider paths never mix with `-os` (`invalid:`). Pure commands and `.cybsh`
script wrappers agree on every transport: `sh("ls -os …")` / `$ ls -os …`
lines go through the same shell choke point as the terminal line (native
`dispatch`, static `runStaticCybshLine`, wasm `dispatch` via `exec_result`),
so scripts see the host listing natively and `unsupported:` anywhere else.
Host access is best-effort:
a denied directory answers `not_found:` with the OS reason (on Android, grant
a folder via Setup → Providers → Local folder when the sandbox withholds
sdcard). The static/Pages build has no host filesystem, so `-os` there
answers `unsupported:` honestly on all three paths — single-command lines
(static guard), chained `&&`/`;` lines and direct `wasm` dispatch calls
(wasm guard) — never a volume listing masquerading as a host one. Agent `bash` runs these same verbs
(cybsh-first in `auto` mode), so the agent touches/moves provider or host
files through `bash` — `read/list/write/edit/grep/glob` stay
volume-contained; prefer `--json` for machine parsing.

## 4b. Agent runs (same async contract)

`POST /api/agent/prompt {configId, sessionId?, prompt} → 202 {jobId}`, poll
`GET /api/agent/jobs/{jobId}` (turns, usage, `pending` approvals),
`POST /api/agent/jobs/{jobId}/approve {approved, answer?}`,
`POST /api/agent/jobs/{jobId}/abort`. Asks park up to 10 min then auto-deny;
`deny` rules always win, even with auto-approve.
`cybsh`: `ai ask "<prompt>" [--config <id>] [--session <id>]` (detached via
the same lock-free intercept), `ai status [job]`, `ai abort [job]`,
`ai sessions`. Provider keys live sealed in `sync_secrets`
(`agent:key:<config_id>`); configs/sessions are plain JSON rows. Transcripts
sync like any file via export → striped placement.

Agent tools: `read/write/edit/list/grep/glob/bash/task/question` (+
`mcp__*`). `grep` is real regex (invalid patterns search literally);
`glob` supports `**` across separators; permission patterns match the bare
argument too (`git *` matches the command, tool prefix not required).

Agent loop hardening (all on by default): rate-limit backoff (3 retries),
doom-loop guard (identical tool call ×3 denied), `length`-finish truncation
notes, abort-checked retries and post-call polling, Anthropic prompt-caching
markers, read-only depth-1 subagents. `POST …/compact` summarizes a session
into a fresh one (old kept for revert). MCP servers (`stdio` local commands,
`http` Streamable) attach per config behind admin role; tools appear as
`mcp__server__tool` under the same permission rules. Tool outputs are
secret-redacted before entering transcripts; crashed MCP children reconnect
once mid-run; project rules load from `AGENTS.md`, `SKILL.md`,
`.cybermanju/rules.md` (8 KiB each, 24 KiB total); approvals can store
"allow always" as an explicit config row; `ai init` runs repo analysis that
writes `AGENTS.md` with the agent's own tools.

Semantic memory (`docs/MEMORY.md`): curated facts + embedding vectors in the
`agent_memories` redb table (same DB file). `memory_recall` / `memory_remember`
tools (permission-gated, subagents denied on remember); pre-prompt auto-recall
injects a bounded 2200-char block; compaction handoffs auto-store; long
memory-less runs set a `memoryHint` nudge. Embeddings via `{base}/embeddings`
(OpenAI dialect; per-config `embeddingModel`, Ollama works offline);
Anthropic degrades to keyword recall, never fails. HTTP:
`GET|POST|DELETE /api/agent/memories`, `POST …/recall`, `GET …/export`
(Hermes-compatible `MEMORY.md` + vectors for sync restore).

## 4c. Static-site cybsh (Pages/WASM transport, no dashboard)

On a static host every `os_exec` line is offered to `src/utils/staticCybsh.ts`
first; chained lines and unknown verbs fall through to the wasm dispatcher
(`crates/os-wasm/src/os.rs`). Verbs answered locally from the local-pc vault
(`sync.secret` signs, `secret:cybsh:key:*` keys, the `.cybermanju` file):

- `quota` — shell volume + browser storage + live per-provider probes (same
  endpoints as `crates/sync/src/quota.rs`); provider push still needs `:3456`.
- `providers`, `oauth status|start`, `disk`, `sync status|list`, `mount`.
- `encrypt|decrypt|keygen` (ChaCha20-Poly1305, vault keys), `compress|decompress` (lz4/zstd/brotli/triple, same `{alg,data:b64}` envelope as the native shell).
- `cp|mv|rm|mkdir` across the merged namespace: plain paths hit the shell
  volume, `/providers/<mountId>/…` hits that mount (same-provider renames,
  cross-provider and provider↔local moves, `-r` for trees, `.keep` markers
  for empty dirs — git mounts cannot hold those natively).
- `ls|cat` for `/providers/…` paths too (`ls /providers` lists mounts plus
  unmounted configs; local operands still fall through to the wasm volume).
  `cd` into providers stays absolute-path only here (the wasm volume owns
  `cwd`); the native shell supports `cd /providers/…` fully.
- `grep|find|head|tail|wc|edit` across the merged namespace too (single files
  via provider reads, `find` lists provider dirs one level, `edit` is
  exact-once with `conflict:` on ambiguity).
- `write` runs in the wasm dispatcher (editor save path, 1 MiB cap).
- `scrub` (BLAKE3 snapshot vs `cache:cybsh:scrub`), `repair` (prunes the
  record), `gc` (dry-run default, `--apply` deletes), `lease`, `echo`, `kill`, `ai`.
- `skill list|show|run|install|panel` (shareable skills under `.cybermanju/skills/` —
  `SKILL.md` manifest + entry `.cybsh`; `install <https-url>` fetches the
  manifest + sibling run script; `run` executes the entry script line by line;
  `panel <slug>` resolves a skill's `panel:` frontmatter to its `skill:<slug>` target).
- `task list|add|done|undone|rm|clear` (todos sharing the Archive TASKS store).
- `oplog plan|conflicts|resolve` (op-log exchange: `plan` prints the
  pull→push→render steps offline; `conflicts`/`resolve` need the dashboard —
  Sync → Conflicts resolves with one click, `POST /api/oplog/resolve`).
- `lan plan|peers|sync` (direct device sync: `plan` prints the
  pull→push→merge→render steps for a peer; `peers`/`sync` need the dashboard —
  Devices → LAN peers → SYNC does the authenticated exchange,
  `POST /api/peers/{id}/sync`).

`sync start|cancel`, the OAuth redirect dance and provider push refuse with
`unsupported:` pointing at the desktop app / Docker image / dashboard server.

## 4c2. P2P + cross-device search (android/ios/windows/linux/docker/wasm)

One grammar everywhere: `<device>@<addr>` (`phone-7@192.168.1.60:3456`,
`vault@https://vault.example.com`). Direct addrs dial live over HTTP(S)
(`GET /api/readyz` probe → owner-scoped `GET /api/oplog/since` pull → signed
`POST /api/peers/{localDeviceId}/sync` push → local merge → owner-scoped
`GET /api/oplog/conflicts` render). Relay
tickets (`relay…`, `iroh:…`, `/p2p/…`) rendezvous through any dashboard set
as `relay_url` — both sides make outbound HTTPS (`POST /api/relay/drop` +
`POST /api/relay/poll`, `POST /api/relay/ack` to clear), so no inbound port
is needed. Every mailbox operation is owner-bound: the caller must own both
devices to enqueue, and only the recipient's owner can poll/ack; presence is
likewise filtered to the caller's active devices. Only the QUIC *datagram*
upgrade stays sequenced. Attested
devices sign every op (`auth::SignedSyncBody`, ML-DSA over the canonical
envelope); unsigned, revoked, mismatched-owner, wrong-device and bad-signature
requests fail closed. The receiver's path id is the sender's device id, and
every signed op must name that device. File bytes ride
sneakernet bundles (`POST /api/sync/bundle/build` with `files:
[{ fileId, name, bytesBase64 }]` — 5 MiB/file, 100 files, 50 MiB total,
every payload BLAKE3-verified on import), larger files ride the sync
backends on next contact. Search fans out per peer (`GET /api/search`,
merged dedup-by-id / max-score-wins via `POST /api/search/federated`).

Op-log pagination treats `since` as an inclusive next-sequence cursor (`0`
includes the first `seq=0` operation); continue from each page's returned
`head`. This preserves existing signed sequence IDs and avoids skipping the
first operation or rows beyond a capped page.

Server-assisted presence covers every target mDNS cannot reach: peers
`POST /api/peers/announce { deviceId, host, port }` once per launch (5 min
TTL) and readers merge `GET /api/peers/known` with the mDNS scan. WebRTC
datachannels rendezvous their offer/answer/ICE through the same relay
mailbox (`kind: offer|answer|ice`).

Human pairing (wormhole-style, no wormhole server): `pair make` mints a
`NNN-word-word` code (`482-oak-river`); the sender drops to the
`pair:<code>` mailbox, the receiver claims it (poll + ack, no replay).
Codes are rendezvous, not PAKE — attestation stays ML-DSA. Bulk bytes ride
erasure-coded swarms: `swarm plan <bytes>` shows the RS layout, then payloads
spray as `kind: swarm-symbol` messages (any `k` of `k+m` reconstruct);
`swarm fountain <bytes> [mtu] [repair]` sprays RaptorQ instead (any K+h
symbols decode — the lossy-path sprayer, natively validated iroh tickets at
`iroh:<endpoint-id>@<relay-url>` carry their own rendezvous, no configured
relay needed). Relay rooms give the sandbox presence +
small-data fan-out: `room join <app> <name>` reads hellos, `room say <app>
<name> <action> [json]` posts typed actions.

| Target | Discovery | Sync | Search | Setup |
|---|---|---|---|---|
| android | presence (`announce`/`known`) — multicast lock blocked | HTTP op-log + relay envelopes while foregrounded | HTTP fan-out, ≤8 peers | same Wi-Fi, announce once per launch, keep app foregrounded |
| ios | presence (`announce`/`known`) — background sockets suspend | HTTP op-log + relay envelopes while foregrounded | HTTP fan-out while foregrounded | Bonjour/multicast entitlement, else presence |
| windows | active scan only (no system mDNS daemon) + presence | LAN + WAN HTTP + relay | local BM25 + peer fan-out | allow UDP 5353 + TCP 3456 at the firewall prompt (`known` needs no rule) |
| linux | active scan (avahi owns 5353, passive answering degrades) + presence | LAN + WAN HTTP + relay | local BM25 + peer fan-out | open UDP 5353 / TCP 3456 in firewalld/ufw (`known` needs nothing) |
| docker | presence on bridge networks | explicit peer URLs + relay | `POST /api/search/federated` server-side | `--network host` for mDNS, else announce the reachable host:port; persist `/data` + `SEARCH_INDEX_PATH` |
| wasm | none (no UDP in sandbox) — explicit peers + server fan-out | relay envelopes + WebRTC datachannels via the relay mailbox | BM25-lite local + `fetch()` to explicit `https://` peers (CORS + mixed-content apply) | explicit `https://` URLs on the CORS allowlist, or server-side fan-out |

Auth: every peer operation requires a token explicitly configured for that
peer. `POST /api/search/federated { peers: [{ baseUrl, deviceId, token }] }`
never forwards the caller's bearer; peers without their own token are skipped.
Direct sync likewise requires the target peer's scoped token plus a local
device signer. Anonymous or unconfigured peers fail closed; no implicit
credential fallback exists. `https://` probes work everywhere (Rust rides
reqwest+rustls, browsers fetch). Device attestation is ML-DSA against the
registry (`sign_peer_ops_for_device` on desktop loads the sealed key from the
vault; Pages uses wasm `ml_dsa65_sign`); raw private seed never crosses IPC.

cybsh: `lan plan|peers|sync`, `wan plan|dial [--relay <url>]`, `pex
announce|known`, `relay drop|poll|ack`, `pair make|drop|claim`, `swarm plan`,
`room join|say`, `oplog plan|conflicts|resolve`;
UI: Devices → P2P matrix (`peer_platform_notes`), Devices → LAN peers →
SYNC (attested ✓ when signed), Devices → Presence, Devices → WAN dial
(relay URL field), OmniSearch federated mode.

## 4d. Interface verbs (cybsh customizes the whole UI)

`theme [<id>|get]` and `ui …` read/write the full interface settings mirror
(`/.cybermanju/theme.json` on the volume, `cybermanju_theme_v1` in the
browser) identically on desktop, Docker, and Pages — 5 themes
(`THEME_IDS` in `src/ui/tokens.ts`, mirrored in `crates/os/src/shell.rs`,
`crates/os-wasm/src/os.rs`, `src/utils/staticCybsh.ts`):
`os-dark` / `os-light` / `os-graphite` (flat macOS set) plus
`plasma-dark` / `plasma-light` (Breeze-like set). Shell style
(`macos` menu-bar+Dock vs `plasma` bottom-panel+Kickoff) is a separate
axis (`cybermanju_shell_style`, `data-ui-shell`):

- `ui theme <id>` — switch theme (legacy pre-remake ids migrate by mode).
- `ui accent <#hex|default> [--for <theme>]` — recolor everything, or one
  theme (`ui accent #ff2d78 --for os-dark`); general wins when set.
- `ui density <compact|comfortable>`, `ui glass <0|solid|2|translucent>`,
  `ui motion <auto|full|reduced>`, `ui glow <on|off>` — every Settings knob.
- `ui get [--json]` — full interface state (also the drift-healer: its
  `ui:` lines re-converge the live UI to the mirror).
- `ui vars [filter] [--json]` — live computed `--ui-*` token values from the
  DOM (live-terminal only; scripts use `ui get`).
- `ui palette [theme] [--json]` — a theme's color table + design language.

Every mutation prints machine `ui: key=value` effect lines
(`ui: theme=…`, `ui: accent=…`, `ui: density|glass|motion|glow=…`,
`ui: accent-for=<theme>:<hex|system>`); the Terminal panel applies them via
`useTheme()`, and `.cybsh` scripts harvest the same keys as structured
effects. Implementations: `ui_cmd`/`theme_cmd` per shell, live application in
`TerminalPanel.applyCybshUiEffects`, tokens in `src/ui/tokens.ts`.

## 5. Durability: scrub / repair / gc / leases

| Task | Route | Notes |
|---|---|---|
| Scrub pass | `POST /api/scrub/run`, `GET /api/scrub/runs` | BLAKE3 re-verify, findings queued for repair |
| Repair | `POST /api/repair/run`, `GET /api/repair/status|tasks|health`, `POST /api/repair/rebuild` | 202 task; rebuild restores catalog after laptop loss |
| GC | `POST /api/repair/gc {dryRun}` | Refcount authority; never deletes referenced chunk |
| Lease | `POST /api/lease/acquire|release`, `GET /api/lease/status[/{scope}]` | Single writer TTL + steal-after-expiry; live lease → `conflict:` |

`cybsh`: `scrub`, `repair`, `gc [--dry]`, `lease status`, `backup export|list|verify`.

## 5b. Vault backup (snapshots + restore)

Snapshots are passphrase-sealed CYBMJ01 containers (`.cybermanju` files —
same bytes on every transport, so a desktop export restores on Pages):

| Task | How | Notes |
|---|---|---|
| One-shot export | `backup export [--to /backups] [--name F] [--keep 7] [--plain]` in cybsh, or Disks → BACKUP → Download | `--to /providers/<id>[/dir]` pushes to any provider; default seals with the vault passphrase (`--plain` records a plaintext export) |
| List / verify | `backup list [dir]`, `backup verify <file> [--deep]` | Header always; `--deep` trial-decrypts (wrong passphrase → `auth:`) |
| Scheduled | Disks → BACKUP schedule card, or `cron add /.cybermanju/scheduled-backup.cybsh` | Writes the managed script (frontmatter owns the expr); runs fire the same `backup export` unattended |
| Download (UI) | Disks → BACKUP → Download snapshot | Tauri/REST return the raw image (sealed in-browser, passphrase never leaves); Pages returns the sealed container; trial-decrypt proves it pre-download |
| Restore (Pages) | BACKUP → Restore wizard → upload/pick → verify → Restore | Replaces the OPFS database + reloads |
| Restore (desktop/Docker) | Wizard verifies, then: stop app → replace `cybermanju.db` → start → `POST /api/repair/rebuild` | The live redb file cannot be hot-swapped; `scripts/backup.sh` stays the host-level path (redb + tantivy + MANIFEST) |
| HTTP export | `GET /api/backup/export` (Admin) | Raw image base64'd, 32 MiB cap — bigger vaults use the shell verb or host script |

## 6. Auth & TLS

- Register is bootstrap-only; role never self-`admin`. Every mutating route checks `Claims{role}` (default `Authenticated`, fail-closed; unknown route without creds → `401`, with creds but unknown → `404` via `is_known_route`).
- First run on Docker/web (every device): open `http://<nas-ip>:3456` — the
  login gate offers **Create the first account** while
  `GET /api/auth/status → { "registrationOpen": true }`, then plain sign-in
  afterwards. Each browser signs in once (JWT in its own localStorage); a 401
  anywhere re-opens the gate instead of spamming fetches.
- Multiple accounts: dashboard users are created by an `admin` in
  Accounts → Users (`POST /api/users`; headless: provision via
  `CYBERMANJU_ADMIN_USERNAME/_PASSWORD` or open registration temporarily
  with `CYBERMANJU_ALLOW_REGISTRATION=1`). Provider OAuth accounts
  (Google/GitHub/GitLab, Connections tab) are a separate system and
  unlimited — including several accounts on the same provider.
- `GET /api/sync/configs` never serializes `token` (absent, not null). Private keys sealed (Argon2id + ChaCha20Poly1305, `sealed:v1:`).
- No TLS in-app (hand-rolled HTTP/1.1). Terminate at Caddy/nginx (HSTS there, not in-app). See `docs/SECURITY.md` for threat model, rotation, OAuth PKCE, rate limits (100 req/60s), parser caps, `security_headers`.
- Shipped proxy (P2-15): `Caddyfile` + `docker-compose.tls.yml` (Caddy 2 sidecar: 80→443, auto-ACME, HSTS + hardening headers, 100 MB body cap aligned to the API limit). `DOMAIN=vault.example.com ACME_EMAIL=you@example.com docker compose -f docker-compose.yml -f docker-compose.tls.yml up -d`. Caddy is the single upstream — no second proxy in front without updating the `X-Forwarded-For` trust notes in `docs/SECURITY.md` §2 (else one-IP false 429s). Behind TLS set `CYBERMANJU_OAUTH_REDIRECT_URI=https://$DOMAIN/api/sync/oauth/<provider>/callback`.

## 7. Health, metrics, backup

- `GET /api/health` (compat shape) + `GET /api/readyz` (redb + index + disk writable) + `GET /api/metrics` (counters).
- Backup while running: `scripts/backup.sh` (redb + tantivy snapshot). Restore = stop, replace files, start, `POST /api/repair/rebuild` if the catalog is suspect, `disk check` each disk.

## 8. Transports

`tauri IPC` (desktop, but `os_*/disk/*` are `REST_FIRST` → `:3456`), `rest` (Docker/web), `wasm` (Pages: localStorage volume + BM25-lite, server ops answer `unsupported:`). Settings shows the active transport.

## 9. Honest limits (browser + crypto)

- **WASM caps:** the Pages pack runs `lz4` + `brotli` natively plus `zstd` and
  `triple` (LZ4→ZSTD→Brotli, the desktop `.cyb3` order) through the bundled
  `@dweb-browser/zstd-wasm` module — same standard frames as the desktop, so
  artifacts open both ways; the dashboard build has no compression endpoint
  at all. Editor + `write` op cap at 1 MiB.
- **ML-KEM label:** the wasm bundle ships X25519 + ML-DSA-65 + ChaCha20-Poly1305
  and no ML-KEM. Keygen for `kyber*`/`hybrid`/`frodokem*` mints X25519 material
  and file encryption is ChaCha20-Poly1305; the panel prints
  `X25519 + ChaCha20-Poly1305 (ML-KEM slot)` so the slot name never implies a
  lattice operation that did not run.
- **CORS:** provider canals only claim `github`, `gitlab`, `googleDrive`
  (anything else is refused up front with `unsupported:`) — live sync for
  remote providers needs the desktop app, Docker image, or dashboard server.
- **Faces:** every detection is engine-labeled (`onnx` / `heuristic-v2` /
  `none`) on all transports — desktop runs SCRFD+ArcFace where `ort` links
  (win/mac/linux + model files) and the labeled skin-segmentation heuristic
  everywhere else (incl. Android); Docker serves `POST /api/faces/detect*`;
  Pages runs the same heuristic in-browser with kv-backed groups. The
  heuristic needs eyes (a pair, or one eye plus a mouth — a bare skin blob
  is a hand, not a face) and says `HEURISTIC` in the panel; ONNX silence
  stays silence.
- **Folder attach:** Chromium remembers picked folders in IndexedDB and
  re-opens them with one click after reload (permission lapses are
  re-granted, never re-picked); desktop uses native dialogs (no permission
  model) and Docker/server paths persist in the DB. Plain-HTTP LAN has no
  File System Access API at all — the vault then lives on the server.
- **tree-sitter:** desktop `parse_text` runs real grammars
  (rust/python/js/ts/go/bash) and reports `"engine": "tree-sitter"`; every other
  language — and the whole Pages build — uses the heuristic parser and reports
  `"engine": "heuristic"`. The editor outline badge shows which one ran.
- **FSA:** the `.cybermanju` open/create/save path needs the File System Access
  API (Chromium). Firefox/Safari get Export/Import fallback instead, and the
  Accounts panel says so before the picker opens.
