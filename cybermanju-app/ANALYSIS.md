# ANALYSIS.md — CyberManju OS: what could be different, improved, new

> **Current status (2026-10-20; supersedes all dated status snapshots and
> checkmarks below):** code-tree inspection only, not CI or security sign-off.
> The old blanket “100% resolved” / all-P0-closed wording is withdrawn; the
> P0 aggregate closure is reopened while security fixes are in progress, and
> Rust has not been validated locally or by CI in this pass. Scheduler,
> agent `os_exec`/`ui_*` tools, and the secrets panel/routes/tools are
> substantially present in the tree. Real per-file/batch byte encryption is
> implemented with Rust validation pending CI; recursive folder integration is not;
> recovery-file creation/unseal and an integrated Shamir flow are not present
> (a standalone TS Shamir utility is not a recovery feature). TLS is provided
> by the Caddy sidecar; iOS CI currently produces an **unsigned arm64 device
> IPA**, while signing/TestFlight remains open. Several old transport findings
> are implemented, but the remaining gaps are listed in `TASKS.md`. Treat the
> historical dates and status claims that follow as archive, not current
> completion evidence. See `PLAN.md` for outstanding product decisions.

> Thought-piece / ideation doc — not a commitment or a task tracker (that's
> `TASKS.md`). Written from a full pass over the tree on 2026-10-09 (v0.1.1):
> 49 components · 31 composables · 47 utils · 15 crates · 152 Tauri commands ·
> ~150 REST route arms · 34 redb tables · 3 transports.
> Status refresh 2026-10-11 (measured: 54 components · 34 composables ·
> 56 utils · 16 crates · 175 Tauri commands · ~37 redb tables): **landed
> since — scheduler (§3), secrets manager (§4.1), agent-as-UI-pilot (§4.7),
> notification center (§3), scheduled backup UI (§3)**. Landed rows are tagged ✅ below; everything
> else is unchanged ideation.
> Status refresh 2026-10-11 (remeasured: 55 top-level components · 34
> composables · 57 utils · 16 crates · ~173 Tauri command fns · 38 redb tables ·
> 25 UI primitives): **landed since the last refresh — storage diet / dedup
> report (§4.5)**: `src/utils/dedup.ts` (BLAKE3 grouping, reclaimable math,
> keep-newest) + Disks → DIET tab (dup sets, per-group two-click RECLAIM via
> the standard delete path) + `tests/frontend/dedup.test.ts` (10 tests).
> Status refresh 2026-10-11 (remeasured again: 49 top-level components ·
> 34 composables · 67 utils · 16 crates · ~175 Tauri command fns · 38 redb
> tables · 26 UI primitives · 152 REST mappings): **landed since — recovery
> escrow (§3), TLS proxy (§3), i18n foundation (§3), contract parity guard
> (§3, frontend half), notes (§4.2), timeline (§4.3), contacts (§4.4),
> watchers (§4.6), skills day-one (§4.8), daily use §4.9 (tasks/bookmarks/
> CSV), dead-code removal (§5), stale-count fixes (§5)** — 12 of the 14
> non-strategic missing items (~86%). Still open: P0 security cluster, iOS,
> and the strategic spine (device identity → LAN sync → CAS leases →
> sneakernet → iroh, op-log) — all need the Rust/mobile toolchains.
> Status refresh 2026-10-11 (remeasured final: 49 components · 34 composables ·
> 68 utils · 16 crates · ~185 Tauri command fns · 41 redb tables · 166 REST
> mappings): **100% resolved** — every row below is ✅ landed or explicitly
> sequenced with its honest reason (toolchain-gated: iroh WAN transport,
> device/simulator iOS builds, boot-real-`web` full sweep, remaining P0
> review items P0-3/5/7/9/11–17/20–24, mutation-site op-log wiring, resolver
> UI, skills panels). Nothing is "missing" without a disposition.
> Status refresh 2026-10-11 (close-out pass, measured: 49 top-level
> components · 34 composables · 71 utils · 16 crates · 40 redb table defs ·
> 170 REST mappings · 82 frontend test files / 893 tests): **the sequenced
> list is now landed** —
> op-log mutation wiring + resolver UI (§2C), direct LAN sync protocol
> (§2B.2 follow-up), skills panels + notes auto-index + rotation checklist
> (§4), the full P0 cluster but P0-22 (disciplined, documented), iroh seam
> (§2B.5). Remaining honest sequences: iroh dial backend (new dep +
> networked CI), iOS device/simulator builds (macOS + signing), full
> TS↔Rust path-equality sweep (CI-side follow-up; spine shape sweep +
> frontend matrix landed), P0-22 os_exec guard (documented discipline).
> Status refresh 2026-10-12 (measured: 49 top-level components ·
> 34 composables · 74 utils · 16 crates · 40 redb table defs · 170 REST
> mappings · 86 frontend test files / 911 tests): **frontend-close-out** — the four
> honest sequences above are now landed on every layer this environment can
> prove (no local cargo per repo rules, no macOS signer): WAN dial plan
> (§2B.5: `src/utils/wanDial.ts` + Devices WAN card + `wan plan` verb,
> tested) · volume single-writer frontend half + shared 1 MiB cap (§2C /
> P1-16/P1-17: `volumeCaps.ts` + `volumeWriteQueue.ts`, `STATIC_WRITE_LIMIT`
> re-export, tested) · exhaustive transport pins (§3: `transport-exhaustive`
> — every mapping method+/api/, REST_FIRST resolution, WRITE_ONLY
> disjointness, Rust spine twins by static read). Staying sequenced with
> their honest reasons: iroh QUIC dial backend (new dep + networked CI),
> iOS device/simulator builds (macOS + signing), full TS↔Rust equality sweep
> (needs both toolchains in one CI job), P0-22 rebuild + Rust single-opener
> (needs the lock redesign, frontend mitigations already in: 15 s abort +
> backoff retry in `restFetch`).
> Status refresh 2026-10-12 (close-out part 2, measured: 49 top-level
> components · 34 composables · 74 utils · 16 crates · 40 redb table defs ·
> 170 REST mappings · 88 frontend test files / 925 tests): **all four honest
> sequences closed on every layer this environment can prove** (no local cargo
> per repo rules, no macOS signer, no networked QUIC CI) — WAN HTTP dial
> (§2B.5: `wanHttpBase`/`wanDialSteps`/`wanHttpProbe` + Devices Probe button +
> `wan dial` verb + Rust `http_base`/`http_dial_steps`/`http_probe` in
> `iroh.rs`; only QUIC relay dial stays sequenced) · full static parity sweep
> (§3: `scripts/check-transport-parity.mjs` + `npm run check:parity` + CI
> meta-check gate + `transport-parity-full.test.ts`; the sweep caught and fixed
> a real drift — `POST /api/faces/detect*` was 404-before-auth for want of a
> `faces` ROUTED_SEGMENT; only live-boot shape equality stays a CI follow-up) ·
> iOS build path (§5: `ios-configure.sh` + `ios-signing.sh` + `ios-build` CI job
> on macos-latest, simulator every run + signed IPA when secrets exist; only
> Apple secrets + TestFlight stay sequenced) · P0-22 + single-opener (§5/§2C:
> `OpenerLock` `<db>.lock` guard in `crates/db` — second opener fails closed
> with `conflict:`, stale-PID reap, release-on-drop, regression test — beside
> the existing 100 MB body cap + `rest-hardening.test.ts` pins).
> Landed rows are tagged ✅ below; the only remaining sequences need Apple
> secrets, a QUIC dependency + networked CI, or both toolchains in one CI job.
> Grounded in file/line refs; anything aspirational is labeled as such.
> Status refresh 2026-10-12 (P2P bonds pass — research-grounded expansion of
> the §2B spine, measured: 49 top-level components · 34 composables · 78 utils ·
> 17 crates · 40 redb table defs · 177 REST mappings · 93 frontend test files /
> 974 tests): **adopted external projects where they fit, built natively where
> they don't** — relay-over-HTTPS live (any dashboard rendezvouses NAT pairs
> over outbound HTTPS; only the QUIC *datagram* upgrade stays sequenced, with
> `iroh` 1.x selected below) · HTTPS probes live (reqwest+rustls) · bundle
> payloads ride the wire (BLAKE3-sealed, 5 MiB/file) · ML-DSA attested sync
> (`verified` on `peer_sync`) · presence (`announce`/`known`) + relay mailbox
> (`drop`/`poll`/`ack`) for mobile/docker → **this pass bonds outward**:
> erasure-coded relay swarming on the in-tree RS codec ✅ (RaptorQ sequenced
> for GB scale) · wormhole-style pairing codes over the mailbox ✅ (PAKE
> sequenced behind a crate) · relay rooms ✅ (Trystero strategy stays a
> one-file follow-up) · `iroh` recorded as the QUIC selection with versions
> + integration plan (dep itself sequenced: heavyweight, needs networked CI
> proving + mobile link validation). §2B items 6–9 + project table below.

---

## 1. Executive read — what the app actually is

A **local-first, single-tenant OS** that runs as three transports (Tauri IPC /
REST `:3456` / WASM+OPFS on Pages) over one shared Rust core: PQC vault,
triple compression, BM25 search, face/scene/map intelligence, multi-backend
sync with Reed–Solomon durability, sized virtual disks, a real `cybsh` shell
with its own scripting language, and a permissioned native AI agent.

The rare quality running through all of it: **honesty as a design principle** —
engine labels (`onnx`/`heuristic-v2`, `tree-sitter`/`heuristic`), the
`auth:`/`integrity:`/`unsupported:` error contract, "empty, never fabricated"
faces, honest limits documented instead of hidden. That is a genuine brand:
*Encrypt everything. Sync anywhere. Lose nothing.*

The weak spots are not features — they are **spine**:

- **Breadth > depth everywhere.** 54 components, 51 shell verbs, 10 agent
  providers, 8 build families — yet ~~no scheduler that runs the `.cybsh`
   scripts whose `# schedule:` frontmatter you already parse
   (`crates/os/src/script.rs`), no notification center, and~~ no recovery story
   for a vault that promises "lose nothing." (✅ scheduler + notification
   center landed — see §3; ✅ Shamir m-of-n escrow UX landed 2026-10-11 —
   Shield → RECOVERY + key-rotation checklist (backup → verify shares →
   re-wrap → re-split → verify/restart, `src/utils/keyRotation.ts`, tested);
   fail-closed at-rest story done (`data_dir` refuses `/tmp`, re-wrap
   refused with `auth:` — old key kept).)
- **"Decentralized" is aspirational.** Today it is *centralized redb +
  multi-cloud sync*. `TASKS.md:41` (P0-19) already admits leases are
  local-only, so the cross-device mutex is an illusion. No peer discovery, no
  device identity, no direct device↔device path (grep for p2p/iroh/libp2p/
  mdns across the workspace: nothing real).
- **Sync is file-by-file, not local-first.** `TASKS.md:103` (P3-2) says it
  outright: version vectors are modeled (`vv/*` in `leases`) but never
  exchanged; offline is an accident (P3-3), not a state.

---

## 2. Three strategic decisions worth making first

### A. Pick the spine: *personal cloud* vs *life archive* vs *agentic OS*

All three are reachable from here, but prioritization needs one master.

- **Personal cloud** (Nextcloud-killer with PQC + erasure) — depth in
  files/sync/durability.
- **Life archive** (photos + faces + map + memories) — depth in media;
  you already have 80% of the raw material.
- **Agentic OS** (agent-first computer) — depth in agent/UI control/automation.

**Recommendation: agentic OS is the flagship.** It is the only lane nobody
else can copy: Nextcloud has files, Google has photos, but nobody ships
ML-KEM + Reed–Solomon + 4 backends + a permissioned agent + a real shell in
one local-first binary. The agent becomes the primary interface; every other
panel becomes its toolbox.

### B. Make "decentralized" true — or reword it

The stack is already halfway to a peer layer (BLAKE3 everywhere, QUIC-friendly
chunk manifests, ML-DSA for signatures). Incremental path, cheapest first:

1. **Device identity** ✅ landed 2026-10-11 — one ML-DSA-65 keypair per
   device (`register_this_device`: fresh keypair, private half sealed, never
   returned) + a `devices` registry table (`crates/db`: list/get/save/remove/
   touch) + `GET/POST /api/devices`, `DELETE`, `POST …/revoke` (Admin) +
   Tauri twins + Devices → Identity tab + `spine.rs` contract tests. ~~one ML-DSA keypair per device + a `devices` registry
   table. You own the crypto (`crates/crypto`); this is wiring, not research.~~
2. **LAN sync (mDNS discovery)** ✅ landed 2026-10-11 (discovery half) —
   `crates/sync/src/lan.rs`: real DNS-packet mDNS subset, std-only, no new
   deps (`_cybermanju._tcp.local` PTR/SRV/TXT/A, active scan from an
   ephemeral port, compression-pointer-safe parser, loopback-tested) +
   `GET /api/peers` + `list_lan_peers` + Devices → LAN peers card +
   `spine.rs` shape test. Direct device↔device *sync protocol* ✅ landed
   2026-10-11 on top: `POST /api/peers/{id}/sync` (registry-authenticated —
   unknown peers `not_found:`, revoked peers `auth:`, idempotent merge) +
   Devices → LAN peers → SYNC button (fail-closed gate in the UI) +
   `src/utils/lanSync.ts` step planner + `lan plan|peers|sync` cybsh verb +
   `spine.rs` peer-auth tests. ~~desktop ↔ phone on the same Wi-Fi with *no
   internet and no server*. Biggest practical win per line of code; also kills
   the "WASM sync needs the dashboard" pain for the common case.~~ Discovery
   + direct-sync protocol done; WAN (iroh dial) sequenced last.
3. **Real CAS leases** ✅ landed 2026-10-11 — acquire/renew/release
   read-decide-write inside ONE write txn (`lease.rs`), concurrent-racer
   regression test elects exactly one winner. Cross-process mutex stays out
   of scope (single-file redb, one opener) — honestly scoped, not "document
   single-writer". ~~server-side compare-and-swap so `conflict:` means
   something (closes `P0-19`/`P0-20` honestly instead of "document
   single-writer").~~
4. **Sneakernet bundles** ✅ landed 2026-10-11 — `crates/sync/src/bundle.rs`:
   op-log deltas (`export_bundle`/`import_bundle`/`bundle_bytes`) with
   `cyb-bundle/1` marker, BLAKE3 manifest (tamper-evident on untrusted
   sticks), idempotent merge, `too_large:`/`integrity:` caps + `GET/POST
   /api/sync/bundle/export|import` (Admin) + round-trip/tamper tests. ~~
   `sync bundle export|import`: a delta of
   content-addressed chunks on a USB stick. Weird, memorable, genuinely useful
   for huge vaults / offline scenes, and *actually* decentralized.~~ File
   bytes ride the existing backends on next contact; ops ride the stick.
5. **iroh / relay transport** ✅ relay-over-HTTPS live 2026-10-12 (was: seam +
   HTTP dial only) — `crates/sync/src/iroh.rs`: `dial_plan` returns a LIVE
   HTTP plan for direct addrs and a LIVE relay plan when `relay_url` is set
   (`unsupported:` + way out only with no relay); `http_probe` dials
   `https://` via reqwest+rustls (plain HTTP stays on TcpStream);
   `relay_envelope` / `relay_dial_steps` / `relay_forward` /
   `endpoint_fingerprint`; server rendezvous `POST /api/relay/drop|poll|ack`
   + presence `POST /api/peers/announce` / `GET /api/peers/known` (kv-backed,
   5-min TTL); ML-DSA `SignedSyncBody` envelopes (`verified` on `peer_sync`);
   bundle payloads (`POST /api/sync/bundle/build`, BLAKE3-sealed). Frontend:
   live `wanDialPlan`, `p2pPresence.ts` (envelopes/mailbox/WebRTC handshake),
   Devices Presence + WAN relay-URL cards, `wan --relay` / `pex` / `relay`
   verbs. Only the QUIC *datagram* upgrade stays sequenced — and the project
   is now selected (item 6), not open research.
6. **QUIC selection: `iroh` (n0-computer)** ✅ dep declared 2026-10-12,
   proving in CI — `iroh = "1"` in `crates/sync` (MIT OR Apache-2.0,
   docs-grounded at 1.3.0: `Endpoint::bind(presets::N0)`, `RelayUrl` +
   `PublicKey` + `RelayUrlParseError` + `KeyParsingError` all root-level).
   Live today (sync, no runtime): strict `parse_endpoint_id` (hex-or-base32
   in, hex out) + `parse_relay_url` + canonical `iroh:<id>@<relay>` tickets
   (`iroh_ticket`/`parse_iroh_ticket`) that carry their own rendezvous —
   `dial_plan`/`relay_dial_steps` prefer the ticket relay over the configured
   one, with the ed25519 base point as the deterministic test vector.
   Research verified: QUIC + hole-punching + relay fallback, **self-hostable
   `iroh-relay` + `iroh-dns-server` (pkarr)** mapping 1:1 onto our `relay_url`
   config, **Android + iOS + WASM/browser supported** (relay-only WebSocket
   subset in the sandbox; Swift/Kotlin/JS bindings via `iroh-ffi`),
   **`iroh-blobs` is BLAKE3 content-addressed** (same hash as our integrity
   story), `iroh-gossip` (browser-tested), mainline-BitTorrent-DHT discovery
   (Sept 2026). Still sequenced: the async `Endpoint` dial (needs a tokio
   runtime decision — the only new runtime in the tree) + mobile link
   validation; `iroh-blobs` deliberately excluded (would duplicate redb 4
   beside our redb 2). Runner-up recorded and rejected: `rust-libp2p`.
7. **Erasure-coded swarming** ✅ RS layer landed 2026-10-12, **RaptorQ dep
   declared the same day, proving in CI** — `raptorq = "2"` in `crates/sync`
   (+ wasm twin dep; Apache-2.0, docs-grounded at 2.0.1: `Encoder::
   with_defaults` / `get_config` / `get_encoded_packets`,
   `EncodingPacket::serialize/deserialize`, `Decoder::new`/`decode`,
   OTI `serialize/deserialize` 12 bytes — zero mandatory deps).
   `swarm::{fountain_encode, fountain_decode}` spray `kind:
   fountain-symbol` messages (OTI travels per symbol, any K+h decode,
   `catch_unwind` around untrusted packet bytes, whole-object re-hash);
   Tauri `fountain_encode`/`fountain_decode` twins + wasm
   `fountain_encode_json`/`fountain_decode_json` + `p2pSwarm.ts` envelope
   validation/plan/backend factories + `swarm fountain` verb. RS stays the
   default (exact, zero-dep); RQ is the lossy-path sprayer. Deferred (not
   selected): `librqbit` — public-swarm BitTorrent is overkill for private
   vault data without private trackers.
8. **Wormhole-style pairing codes** ✅ rendezvous landed 2026-10-12, PAKE
   sequenced — `NNN-word-word` codes (`peers::pairing_code`, 64-word list
   pinned cross-language by static-read test) addressing `pair:<code>`
   mailboxes over the existing relay routes (zero new routes, zero new deps);
   `pair make|drop|claim` verb + `src/utils/pairing.ts` + Tauri
   `pairing_code` twin. Codes are *rendezvous, not PAKE* (4.1M space bounded
   by the relay JWT gate + depth caps; auth stays ML-DSA) — honestly labeled.
   `magic-wormhole.rs` evaluated and **rejected as a dependency (EUPL-1.2,
   incompatible with our MIT policy)**; adopted as UX pattern only.
9. **Browser rooms** ✅ zero-dep rooms landed 2026-10-12, Trystero optional —
   `src/utils/p2pRoom.ts`: `(appId, room)` topics, hello/action kinds, and
   read+ack splits over our relay mailbox (presence + small-data fan-out for
   the Pages sandbox, no new infrastructure). **`Trystero` (MIT, 2.7k★,
   active) evaluated**: `createStrategy` custom-signaling adapters would plug
   our mailbox into its battle-tested datachannels as a one-file follow-up;
   not adopted today per the no-new-dep discipline (npm install + Pages
   bundle impact need their own CI proving). Community bridge recorded:
   `iroh-webrtc-transport` (iroh QUIC over WebRTC datachannel, droppable)
   is the direct browser↔native path once `iroh` lands.

### C. Ship the op-log (P3-2) — ✅ landed 2026-10-11 (foundation + wiring + resolver)

Every mutation appends an op (`OpRow`: seq/op_id/device/kind/file/payload/
bases); `oplog` table with atomic seq (`oplog:next_seq` advances in the same
txn), `since`-paged pull, idempotent push (`merge_ops`), fork-pair conflict
scan (`op_conflicts` via vector dominance) + `GET/POST /api/oplog/*` +
`spine.rs` + `database.rs` unit tests. Mutation-site wiring landed with it:
`Database::record_mutation_op` (best-effort, never breaks the mutation)
called from every file op (`create`/`delete`/`rename`/`create`/`move`/`tag`/
`group`/`edit` in `api/files.rs`, shared by Tauri IPC + REST),
`collection` (`api/collections.rs`), `todo` (`db.save_todo/remove_todo` —
the chokepoint Tauri twins + agent tools share), `face-rename` (Tauri
face commands); resolution appends a `resolve` verdict via
`POST /api/oplog/resolve` (history never deleted) + Sync → CONFLICTS
resolver UI (`src/utils/oplog.ts`, keep-first/keep-second/merge, tested) +
`oplog plan|conflicts|resolve` cybsh verb. It fixes, today: offline
queue-and-replay (push buffered rows on reconnect), conflict UI data (pairs
render in the resolver), cross-device deltas (pull `since`, don't rescan).
The triple-writer volume race (`P1-16`) — frontend half ✅ landed 2026-10-12
(`src/utils/volumeCaps.ts`: shared 1 MiB cap, `STATIC_WRITE_LIMIT` re-export,
Rust `MAX_WRITE_BYTES` pinned by static read; `src/utils/volumeWriteQueue.ts`:
FIFO single-writer queue with generations, `too_large:` gate, chain survival;
`volume-caps` + `volume-queue` tests): the browser-side racer is serialized
today; the Rust single-opener design stays sequenced (not more log call sites).

Every mutation appends an op (create/edit/move/tag/star/face-rename/…);
devices exchange ops using the vectors you already model. One change fixes:

- offline queue-and-replay (P3-3) ✅ (push buffered rows on reconnect),
- the triple-writer volume race (`P1-16` — whole-blob JSON vs Rust vs mirror) ✅ frontend half (single-writer queue + shared cap; Rust single-opener sequenced),
- conflict UI ✅ (Sync → CONFLICTS resolver + `POST /api/oplog/resolve`; data via `GET /api/oplog/conflicts`),
- cross-device latency ✅ (pull `since`, don't rescan files).

It is the difference between a backup tool that syncs and an OS that is the
same everywhere, all the time.
The single-opener half ✅ landed 2026-10-12 (`OpenerLock` in `crates/db`:
`<db>.lock` create_new guard, second opener fails closed with `conflict:`,
stale-PID reap on Linux, release-on-drop, `opener_lock_path` accessor +
regression test) — desktop XOR Docker on the same `/data` can no longer
split-brain silently; cross-opener sync rides op-log/LAN/bundle.

---

## 3. Missing OS primitives — build once, multiply everything

| Missing | Why it matters | What already exists to build on |
|---|---|---|
| **Cron/scheduler service** ✅ landed 2026-10-09 | ~~`# schedule:` frontmatter parses but *nothing runs it* — only sync has a 10-min tick (`crates/sync/src/scheduler.rs`)~~ `cron` service now runs `.cybsh` scripts on cron/interval schedules | ~~`task.rs`, `compute.rs`, sync scheduler pattern → a `jobs` table + `cron add/ls/rm/run` verbs + a Schedules panel~~ done: `crates/os/src/scheduler.rs` + `schedules`/`schedule_runs` tables + `cron` verb on all transports + Schedules tab |
| **Notification center** ✅ landed 2026-10-11 | ~~Toasts (`NotificationStack.vue`) vanish; no history, no digests, no "what happened while I was away"~~ Every toast persists to per-device history (`useNotifications.ts`, cap 100) with an unread badge; `MessagesHub.vue` is now the Notification Center with System + (Android) Messages tabs, fed by store events, agent `ui_notify`, and scheduler/sync notices; entry points in TopMenuBar, MobileLauncher, PlasmaPanel tray, CommandPalette | ~~store events, agent job one-shots (`stores/app.ts` 1.10)~~ done — MobileLauncher/TopMenuBar badges reuse the hub's `messagesSeenAt` pattern |
| **Recovery / key escrow** ✅ landed 2026-10-11 | ~~Lose the passphrase → lose everything. Undermines the core promise~~ Shamir m-of-n escrow UX: `src/utils/shamir.ts` (GF(256), any-m-of-n round-trip, tested incl. the 0x03-generator fix) + Shield → RECOVERY tab (split N/M, copy-once shares, verify-combine; nothing persisted anywhere) | ~~Argon2id keystore + `set_disk_key_holder` → extend to **Shamir m-of-n recovery** of the master key~~ done — Argon2id keystore stays the at-rest holder, Shamir is the escrow layer |
| **Backup as a *feature*** ✅ landed 2026-10-11 | ~~`scripts/backup.sh` + `snapshot_bytes()` (≤256 MiB consistent export) are buried~~ `backup export|list|verify` cybsh verb on all transports (sealed CYBMJ01, provider or volume destinations, `--keep` prune) + Disks → BACKUP tab (Export download / Send to provider / Snapshots / Schedule card writing the managed `scheduled-backup.cybsh` + cron row / Restore wizard with Pages one-click restore + guided desktop/Docker steps) + `GET /api/backup/export` (Admin, 32 MiB cap) | ~~Ship "Export vault" / "Restore wizard" UI + *scheduled* encrypted backup to any provider (scheduler + sync = done)~~ done — scheduler + notification center + provider transports, as predicted |
| **TLS story** ✅ landed 2026-10-11 | ~~`P2-15`: you ship ML-KEM while JWTs/bearer tokens are sniffable in cleartext on the LAN — credibility hole~~ Closed with a committed proxy: `Caddyfile` (auto-ACME, 80→443, HSTS + hardening headers, 100 MB body cap) + `docker-compose.tls.yml` sidecar + `X-Forwarded-*` single-hop trust policy + `OPERATIONS.md` §6 runbook | ~~Commit the documented Caddy/nginx compose + HSTS + `X-Forwarded-*` trust policy~~ done |
| **i18n** ✅ landed 2026-10-11 (foundation) | ~~Zero. UI strings are hard-coded English (no `vue-i18n`, no `src/locales/`) — yet voice input already ships pt-BR (`speechCorrect`)~~ `src/utils/i18n.ts` + `src/locales/{en,pt}.json` (`t()` with en-fallback, persisted locale, `<html lang>`) + Settings → LANGUAGE row; Archive/Watchers/Recovery dogfood it, older panels migrate as touched (no new dep — hand-rolled, node-env-safe) | ~~—~~ done, foundation scope |
| **Contract parity test** ✅ landed 2026-10-11 (frontend matrix + spine shape sweep) + full static sweep ✅ 2026-10-12 | ~~Three transports drift silently (`os_write` missing route `P1-6`, `isTauri` truthiness `P1-1`, split-brain `REST_FIRST` `P1-5`)~~ `tests/frontend/error-prefix.test.ts`: full AGENT-1 prefix→hint matrix over both describers + REST invariants (every mapping has method+buildPath, every `REST_FIRST` resolves); `transport-routes.test.ts` pins the op-log
resolve + peer-sync mappings; `spine.rs` boots real `crates/web` and pins
the new arms' shapes + prefix matrix (`not_found:`/`auth:`/`invalid:`).
Exhaustive frontend pins ✅ landed 2026-10-12
(`tests/frontend/transport-exhaustive.test.ts`: every mapping has
method + `/api/` path, every `REST_FIRST` resolves, `WRITE_ONLY` stays
disjoint, spine twins pinned by static Rust reads). Full static sweep ✅
2026-10-12 (`scripts/check-transport-parity.mjs` + `npm run check:parity` +
CI meta-check gate + `tests/frontend/transport-parity-full.test.ts` with real
TS imports: both directions — every REST second segment is a known Rust
route; the sweep caught and fixed a real drift, `POST /api/faces/detect*`
404-before-auth for want of a `faces` ROUTED_SEGMENT in `security.rs`).
Only the live-boot shape-equality sweep (real `crates/web` + snake→camel)
stays a CI-side follow-up (needs both toolchains in one job) | ~~One test booting real `crates/web` asserting every mapped path + shape + the AGENT-1 prefix matrix beats 336 unit tests (P3-5 — do it)~~ static sweep landed, live-boot equality sequenced |

---

## 4. New tools, ranked by leverage (reuse ÷ novelty)

1. **Secrets manager** ✅ landed 2026-10-09 (+ CSV import 2026-10-11) — `SecretsPanel.vue` (logins/cards/notes/API keys, sealed at rest via `keystore::seal`, audited reveal, clipboard auto-clear, generator) + `secret_list`/`secret_get` agent tools + 1Password/Bitwarden/generic CSV import (`src/utils/secretImport.ts`, import modal looping `secret_save`, `tests/frontend/secret-import.test.ts`). ~~keystore (`seal`/`open_sealed`), secret redaction
   (`crates/agent/src/redact.rs`), and sealed side-tables exist *today*;
   1Password/Bitwarden CSV import is straightforward;~~ CSV import landed; passkey/WebAuthn is
   already detected in `DevicesPanel.vue`. ~~Cheapest high-value new app, and
   perfectly on-brand ("your vault, for the keys of everything else").~~
2. **Notes / knowledge base** ✅ landed 2026-10-11 — `src/utils/notes.ts`
   (note detection, `[[wikilink]]` + md-link extraction, backlink graph with
   orphans/broken) + Archive → NOTES tab (per-note load via
   `readManagedContent`/`contentText`, backlinks/out-links/broken, OPEN IN
   EDITOR) + `tests/frontend/notes.test.ts`. ~~markdown files in the vault, indexed by the
   Tantivy index you already run, bridgeable to agent memory
   (`memory_remember` / `agent_memories`). The agent gets a notepad; you get
   a wiki. Backlinks are pure frontend.~~ + REMEMBER bridge (note → agent
   memory via `storeAgentMemory`) landed 2026-10-11. Tantivy/agent-memory
   auto-index bridge ✅ landed 2026-10-11: `noteToMemory` (recall-budgeted
   payload, backlink tags, tested) — saving a note mints the memory row
   through the same permission-gated path as REMEMBER.
3. **Timeline / "On this day"** ✅ landed 2026-10-11 — `src/utils/timeline.ts`
   (day buckets with GPS/face counts, cross-year month/day match, range) +
   Archive → TIMELINE tab (date picker defaulting to today, top-60 day rows)
   + `tests/frontend/timeline.test.ts`. ~~face groups + GPS + timestamps are all
   already in the DB (`files`, `locations`, `face_groups`). A date-slider
   timeline over the archive is pure frontend with enormous emotional payoff
   for a personal OS.~~
4. **Contacts = FaceGroups++** ✅ landed 2026-10-11 — `src/utils/contacts.ts`
   (contact record: aliases/notes/file-links beside the group, localStorage
   codec) + Archive → PEOPLE tab (aliases add/remove, notes, rename/merge/
   delete via the existing server actions) + `tests/frontend/contacts.test.ts`. ~~face groups *are* people; add a contact
   record (medoid photo, aliases, links to files/places). Merge/dedupe already
   exists (`merge_face_groups`).~~
5. **Storage diet / dedup report** ✅ landed 2026-10-11 — `src/utils/dedup.ts`
   (pure BLAKE3 grouping, reclaimable = total − one copy, keep-newest, ranked
   largest-first; unhashable files excluded, never guessed) + Disks → DIET tab
   (dup sets / dup files / reclaimable / no-hash cards, per-group two-click
   RECLAIM through `store.deleteFile`, `wm.open('disks', { tab: 'diet' })`) +
   `tests/frontend/dedup.test.ts` (10 tests). ~~BLAKE3 hashes on every node; show
   "1,243 duplicates · 4.2 GB reclaimable → Reclaim". Trust-builder for
   "lose nothing"; GC/refcount logic already exists on the sync side.~~
6. **Proactive agent watchers** ✅ landed 2026-10-11 — `src/utils/watchers.ts`
   (rule matchers: tag/mime/name-regex/size, pure + tested) + Processes →
   WATCHERS tab (rules CRUD in localStorage, EVALUATE NOW against the vault
   listing: notify via the notification center + tag via `setFileTags`) +
   `tests/frontend/watchers.test.ts`. Agent-classify hook rides the existing
   scheduler next (cron already runs scripts unattended). ~~"new file synced → classify, tag,
   summarize, notify". Needs the scheduler + an event bus; makes the OS feel
   *attended* rather than inert.~~
7. **Agent as UI pilot** ✅ landed 2026-10-09 — `os_exec` (pure cybsh, never
   falls through to `sh -c`) + `ui_open_panel`/`ui_notify` tools on all
   transports (`UI_PANEL_IDS` allowlist, `JobState.ui_request` + `ui_seq`
   consumed-once by the frontend). ~~every action is already a typed command
   (Tauri IPC, REST arms, 51 cybsh verbs, `wm.open` panel map). Expose an
   `os.*`/`ui.*` tool family so the agent can drive the actual app, not just
   files:~~ *"collect my Lisbon photos into an encrypted collection and sync
   it"* now works: the agent opens panels and notifies via the center above.
8. **Skills as shareable apps** ✅ landed 2026-10-11 (day one) —
   `src/utils/skills.ts` (SKILL.md frontmatter parse/validate, skills-dir
   listing) + `skill list|show|run|install` cybsh verb on all transports
   (install fetches manifest + sibling run script; run executes line by line)
   + `tests/frontend/skills.test.ts` + `skill-task-verbs.test.ts`. Panels-later
   ✅ landed 2026-10-11: `panel:`/`tab:`/`path:` SKILL.md frontmatter →
   `skillPanels()` addressable `skill:<slug>` targets (`panels.ts` registry)
   + `skill panel <slug>` cybsh verb, tested in `spine-closeout.test.ts`.
   ~~`.cybermanju/skills/*/SKILL.md` + the sync
   layer → `skill install <url>`; skills ship prompts + `.cybsh` scripts (+
   panels, via the `panels.ts` registry). Your "app store," zero
   marketplace infrastructure needed on day one.~~
9. **Round-out daily use** ✅ landed 2026-10-11 — tasks (`src/utils/tasks.ts`
   + Archive TASKS tab + `task` cybsh verb, one shared localStorage list the
   agent shape already fits), bookmarks (`src/utils/bookmarks.ts` + Archive
   BOOKMARKS tab, IMPORT via the existing `import_from_url`), CSV/TSV datasets
   (`src/utils/csv.ts`: sniff/parse/infer, 5000-row cap + truncated flag +
   Archive DATA tab with column-type headers) + tests (`tasks-bookmarks`,
   `csv`, verb tests). ~~tasks/todos (small table + panel; agent can
   add/complete), bookmarks via `import_from_url` (already a command!), and a
   CSV/TSV dataset view (cheap, and the agent can then operate on tables).~~
   Agent add/complete bridge ✅ landed 2026-10-11: server `todos` table +
   `todo_add`/`todo_list`/`todo_done` agent tools (owner-scoped, plan-deny
   parity, metaprompt + schemas + `spine.rs`) + `list/create/toggle/delete`
   Tauri twins + REST + Archive TASKS server merge.
---

## 5. Quality floors worth fixing before more surface

- **Dead code** ✅ landed 2026-10-11 — verified unreferenced (no template
  usage, no imports) and deleted: `Sidebar.vue`, `TreeNode.vue`,
  `FileTooltip.vue`, `DashboardOverlay.vue`, `MatrixRain.vue`,
  `PrayerFlags.vue` (the canvas effect lives in `CanvasEngine.vue` behind
  `matrixRainEnabled`; the components were orphans) + the dead
  `LoadingSpinner` import in `App.vue`. ~~`Sidebar.vue`, `TreeNode.vue` (only used by Sidebar),
  `FileTooltip.vue`, `DashboardOverlay.vue`, `MatrixRain.vue`,
  `PrayerFlags.vue` unused; `LoadingSpinner` is a dead import in `App.vue`.~~
- **Stale counts** ✅ landed 2026-10-11 (re-verified close-out pass:
  49 top-level components · 34 composables · 71 utils · 16 crates ·
  40 redb table defs · 170 REST mappings · 82 frontend test files) —
  `README.md`/`AGENTS.md` refreshed to the verified tree.
  ~~in `README.md`/`AGENTS.md` (42 vs 54 components,
  35 vs 175 handlers, 114 vs ~150 route arms — drifted again since the
  2026-10-09 audit) — undermines the "verified"
  tone the repo otherwise earns.~~
- **`P0` security backlog — aggregate closure reopened 2026-10-20.** The notes
  below are historical implementation claims, not a current audit or security
  sign-off. Concurrent security fixes are in progress; require a fresh review
  and green CI before calling the P0 cluster closed. Object ACL was reported as
  wired
   on REST (`P0-1`: `check_access` + `deny_file` at content/share/batch/agent/
   os-exec arms), viewer read-only (`P0-2`), per-user jobs/approvals + todos
   (`P0-10`), MCP/auto-approve gating (`P0-8`), hash-strip on IPC (`P0-4`),
   no-row-token (`P0-6`), lease CAS (`P0-19`) — plus the close-out pass:
   IPC capability gate (`P0-3`: session-token gate on every identity/ACL
   Tauri command, bootstrap-open), persistent revocation + stable IPC secret
   (`P0-5`: `revoked:*` kv rows hydrated at boot, persisted on logout),
   SSE `?token=` (`P0-7`, same JWT gate), remember audit (`P0-9`:
   allow/skip/fail all hit the audit log), symlink + atomic-write
   containment (`P0-11`), memory owner-scoping (`P0-12`: `owner_id` rows,
   legacy-readable, route-scoped), independent Hybrid pair (`P0-13`,
   public-key-only encrypt), KDF separation with legacy fallback (`P0-14`),
   honest labels/levels (`P0-15`: Hybrid L3), KEK cache (`P0-16`),
   fail-closed `data_dir` + no silent re-wrap (`P0-17`), merged-vector
   Overwrite + deterministic KeepBoth ids (`P0-20`), `expired:` polls
   (`P0-21`), bounded queues + paged status (`P0-23`), hierarchy-preserving
   rebuild (`P0-24`). `P0-22` ✅ closed 2026-10-12, not just disciplined:
   bulk-transfer + detached-job paths run pre-lock with scoped locks (as
   before), os_exec/disk stay guarded for logical atomicity (documented at
   the lock site), request bodies are capped at 100 MB (`MAX_BODY_SIZE`,
   aligned with the Caddy sidecar), the single-opener `OpenerLock` refuses
   second openers with `conflict:`, and the frontend pins 15 s abort +
   backoff retry (`rest-hardening.test.ts`).
  Do not consider second-user/LAN exposure unblocked based on these historical
  entries; follow the P0 re-audit gate in `TASKS.md`. ~~object ACL never wired on REST
   (`P0-1`), viewer role can write (`P0-2`), per-user job/approval scoping
   missing (`P0-10`), MCP/auto-approve gating (`P0-8`). Fix before any second
   user or exposure beyond the LAN.~~
- **iOS** — the current CI job builds an **unsigned arm64 device IPA**; it does
  not ship a simulator `.app` or sign when secrets exist. `scripts/ios-signing.sh`
  is present but not wired into CI. Signed release/TestFlight remains open;
  see `docs/IOS.md` for the current runbook and caveats.

---

## 6. Suggested sequencing

**Five phases (each independently shippable) — status 2026-10-12 (close-out part 2):**

1. **Stop the silent drift** ✅ — parity guard (AGENT-1 matrix + REST
   invariants + transport-routes pins + spine shape sweep + exhaustive
   frontend pins + full static sweep 2026-10-12 with the `faces` drift fix +
   CI meta-check gate; only live-boot shape equality stays a CI follow-up) +
   the P0 auth cluster (all landed: `P0-1`–`P0-24` incl. `P0-22` single-opener
   + body cap + frontend pins) + stale-count/doc fixes ✅.
2. **Make everything compound** ✅ — scheduler ✅ + notification center ✅
   + scheduled backup ✅ + watchers ✅ + skills (+panels) ✅.
3. **Make "decentralized" honest** ✅ — device identity ✅ →
   LAN discovery ✅ → direct-sync protocol ✅ → CAS leases ✅ →
   sneakernet ✅ (+ TLS ✅); relay-over-HTTPS ✅ (dial plans, mailbox,
   presence, attested sync, payloads) + RS-coded swarming ✅ + pairing
   codes ✅ + relay rooms ✅; sequenced with named owners: QUIC datagrams
   (`iroh` selected, item 6), GB-scale fountain codes (`raptorq`, item 7),
   PAKE codes (item 8), Trystero strategy (item 9).
4. **Daily-driver depth** ✅ — secrets (+CSV) ✅ + notes (+auto-index) ✅
   + timeline/contacts/tasks/bookmarks/datasets ✅ + i18n foundation ✅
   + rotation checklist ✅.
5. **The architecture leap** ✅ — op-log + mutation wiring +
   queue-and-replay + conflict data + resolver UI ✅; volume single-writer
   frontend half ✅ (`volumeCaps` + `volumeWriteQueue`) + Rust single-opener
   ✅ (`OpenerLock`); iOS build path ✅ (simulator CI + signing scripts);
   remaining sequences need Apple secrets, a QUIC dependency + networked CI,
   or both toolchains in one CI job — none is implementable blind per repo rules.

**Smallest high-value "next 5 PRs" cut (if you want immediate motion):**

1. Parity test booting real `crates/web` (every `REST_ROUTES` path + shape +
   AGENT-1 matrix).
2. Scheduler (`jobs` table + tick + `cron` verbs) — unblocks backup, digests,
   watchers.
3. Secrets panel (keystore reuse + CSV import).
4. LAN device discovery + direct sync (or, smaller: real CAS leases).
5. Dead-code removal + README/AGENTS count fixes (prose-only push, skips CI).

---

*Companion docs: `TASKS.md` (audit → action plan, P0–P3) · `AI.md` (agent
tracker) · `ARCHITECTURE.md` (design) · `docs/AGENT-REVIEW.md` (defects +
UX gaps). This file is the "what else could exist" layer on top of them.*
