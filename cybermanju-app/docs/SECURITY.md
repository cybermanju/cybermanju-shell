# Security

Threat model, transport story, key management and operational hardening for
CyberManju OS. Owner: **AGENT-3** (`AGENT-3.md`).

---

## 1. Threat model

The product is a *decentralized PC*: a local library of files, identities and
encryption keys that also syncs ciphertext to third-party providers.

**Assets**

| Asset | Where it lives | Why it matters |
|---|---|---|
| File contents | Local disk (plain) + provider (encrypted) | The user's data |
| Encryption private keys | `encryption_keys` table, sealed at rest | Compromise = total loss |
| JWT signing secret | `CYBERMANJU_JWT_SECRET` / `<data dir>/jwt_secret` (0600) | Compromise = forged sessions |
| Master passphrase | `CYBERMANJU_MASTER_PASSPHRASE` / `<data dir>/master.passphrase` (0600) | Compromise = unseals keys at rest |
| Provider tokens | `sync_configs.token` + `<data dir>/oauth_credentials.enc` | Compromise = access to the user's cloud accounts |
| User password hashes | `users.password_hash` (Argon2id) | Compromise = offline cracking |

**Trust boundaries**

1. **Browser ↔ web server** — untrusted. The dashboard binds `127.0.0.1` by
   default; Docker binds `0.0.0.0` behind a proxy. Everything crossing this
   boundary is authenticated (JWT), role-checked, size-capped and
   rate-limited.
2. **App ↔ provider API** — semi-trusted. The app sends *ciphertext* only
   (AGENT-2 encrypts before upload; AGENT-3 seals private keys at rest), and
   the bearer token is the only thing a provider can correlate to an account.
3. **Server ↔ P2P peer** — untrusted destination. Federated search and direct
   sync send only a token explicitly assigned to that peer; the caller's
   inbound `Authorization` token is never reused or forwarded. Treat each
   configured peer token as a secret scoped to that peer URL. Sync writes are
   accepted only with an ML-DSA signature from an active registered device;
   the signed ops must all name that device and its registered algorithm.
   Op-log pulls, conflict scans, resolution, presence and relay mailboxes are
   scoped to active devices owned by the authenticated account (admins may
   audit where the route permits it).
4. **Process ↔ disk** — trusted at runtime, untrusted at rest. Anything
   persisted that is secret is either 0600 (JWT secret, master passphrase) or
   encrypted with an Argon2id-derived key (private keys, OAuth credentials).

**In scope / out of scope**

*In scope:* remote unauthenticated attacker against a published Docker
deployment; a malicious `Origin`; a malicious logged-in non-admin user; a
provider that is fully compromised; an attacker with read access to the data
directory but **not** the master passphrase.

*Out of scope:* an attacker with both the data directory and the master
passphrase (they own the machine); physical DMA/side-channel attacks; supply
chain compromise of a dependency; a malicious desktop user running as the
same OS account (the Tauri IPC surface is trusted by design).

The desktop signing command accepts a device id and op list, loads the sealed
seed from the local vault, verifies the active local device registry binding,
and never accepts or returns raw seed material over IPC.

---

## 2. Transport: there is no TLS

The API is a **hand-rolled HTTP/1.1 server** (`crates/web/src/lib.rs`) with no
TLS. That is acceptable for the default desktop binding (`127.0.0.1`) and is
**not** acceptable for the published Docker port.

Consequences:

* On localhost, `Authorization: Bearer …` never leaves the machine.
* On `0.0.0.0:3456` (Docker), every request — credentials, JWT, file bytes —
  crosses the network in cleartext, and the browser will also refuse to send
  `Secure` cookies / may downgrade mixed content.

**Never expose port 3456 directly to the internet.** Terminate TLS in front.

### 2.1 Caddy (recommended)

```caddyfile
drive.example.com {
    reverse_proxy 127.0.0.1:3456

    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options "nosniff"
        Referrer-Policy "no-referrer"
        X-Frame-Options "DENY"
        Permissions-Policy "camera=(), microphone=(), geolocation=(), payment=()"
    }

    # Belt and braces: the app already sets a CSP, override only if you host
    # the SPA from a different origin.
}
```

Caddy obtains and renews a certificate automatically.

### 2.2 nginx

```nginx
server {
    listen 443 ssl http2;
    server_name drive.example.com;

    ssl_certificate     /etc/letsencrypt/live/drive.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/drive.example.com/privkey.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;

    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains; preload" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "no-referrer" always;
    add_header X-Frame-Options "DENY" always;

    # The API accepts bodies up to 100 MB; keep nginx in step.
    client_max_body_size 100m;

    location / {
        proxy_pass http://127.0.0.1:3456;
        proxy_set_header Host              $host;
        proxy_set_header X-Real-IP         $remote_addr;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

server {
    listen 80;
    server_name drive.example.com;
    return 301 https://$host$request_uri;
}
```

Set `CYBERMANJU_OAUTH_REDIRECT_URI=https://drive.example.com/api/sync/oauth/<provider>/callback`
when you terminate TLS at a proxy (see §5).

---

## 3. Authentication

### 3.1 Passwords

* Hash: **Argon2id**, parameters pinned to OWASP guidance —
  `m=19456 KiB, t=2, p=1`, 32-byte tag, random 16-byte salt
  (`crates/web/src/api/users.rs`).
* Verification rejects any hash that is not an `$argon2…` PHC string, so a
  substituted value can never be coerced into a different algorithm.
* **Legacy migration.** Older desktop builds stored an *unsalted* BLAKE3
  hex digest. Those hashes are accepted once, compared in constant time, and
  immediately rewritten as Argon2id — by both the HTTP and Tauri transports,
  which now share `api::users::authenticate`.

### 3.2 Sessions (JWT)

* HS256, 24 h expiry, `sub` / `user_id` / `role` / `iat` / `exp` / `jti`.
* The signing secret is resolved once, in this order:
  1. `CYBERMANJU_JWT_SECRET` — 64 hex chars are used raw, anything else is
     stretched with BLAKE3 to 32 bytes;
  2. `<data dir>/jwt_secret`, generated on first run with mode `0600`;
  3. a per-process random secret (only when no data directory can be
     resolved — **sessions then die on restart**).
* **Rotation:** delete `jwt_secret` (or set `CYBERMANJU_JWT_SECRET` to a new
  value) and restart. Every outstanding token is invalidated immediately.
* **Logout:** `POST /api/auth/logout` revokes the presented token's `jti` for
  the remainder of its lifetime. The denylist lives in memory, so a restart
  clears it — combined with rotation this covers every case.

### 3.3 Authorization

`security::required_role(method, segments)` is the route → role table; the
default is **Authenticated** (fail-closed).

| Requirement | Routes |
|---|---|
| Public | `GET /api`, `GET /api/health`, `GET /api/readyz`, `GET /api/metrics`, `POST /api/auth/login`, `POST /api/users/login`, `POST /api/users/register`, `GET|POST /api/shared/*`, `GET /api/sync/oauth/{provider}/callback` |
| Admin | `POST /api/users`, `DELETE /api/users/{id}`, `POST /api/users/{id}/role`, `DELETE /api/trash`, `GET /api/share-links`, `DELETE /api/share-links/{id}`, `DELETE /api/sync/configs/{id}` |
| Authenticated | everything else |

Anything that fails authentication answers `401`; anything authenticated but
under-privileged answers `403`.

### 3.4 Registration and the first administrator

Open registration is **bootstrap-only**: it succeeds while the user table is
empty, or when an operator sets `CYBERMANJU_ALLOW_REGISTRATION=1`. The public
path can never mint an `admin`.

Because that would otherwise deadlock a headless deployment, Docker-style
installs provision the first administrator out of band:

```bash
CYBERMANJU_ADMIN_USERNAME=admin
CYBERMANJU_ADMIN_PASSWORD=<long random password>
```

Both must be set; the account is created (or promoted/reset) at startup.
Remove the variables after the first successful login.

Desktop (Tauri) installs use the local IPC path, which is a trusted process
and may create administrators directly.

### 3.5 Brute force

* Per-IP fixed-window rate limiting: `100 req / 60 s`
  (`security::enforce_rate_limit`).
* Per-account login backoff: the 5th consecutive failure locks that username
  for 30 s, doubling per further failure up to 15 min. Lockout state is
  cleared by a successful login.
* Failed and successful logins both write an audit event.

---

## 4. Secrets at rest

| Secret | Protection |
|---|---|
| JWT secret | `0600` file / env |
| Master passphrase | `0600` file / env |
| Native/server encryption private keys | Argon2id-derived key + ChaCha20Poly1305 (`sealed:v1:` prefix) |
| Browser/WASM encryption private keys | Unwrapped `secret:key:<id>` records in the same vault as ciphertext; not protected against a complete vault copy |
| OAuth credentials | Argon2id-derived key + ChaCha20Poly1305, `0600` file |
| Provider tokens (`SyncConfig.token`) | never serialized to a client (`skip_serializing`) |
| User passwords | Argon2id |

Nothing secret is ever returned by an API response: `SyncConfig.token`,
`CloudAccount.token` and every `OAuthCredentials` secret field are
`skip_serializing`, and `/api/users` strips `passwordHash` before it leaves
the process.

**If the master passphrase is lost, sealed private keys and stored OAuth
credentials cannot be recovered.** Back up `<data dir>/master.passphrase`
separately and securely; a database backup without the matching passphrase is
not sufficient. Conversely, a passphrase copy alone does not back up the
database, keystore handles, or OAuth credential file.

### 4.1 Backups and recovery limits

`scripts/backup.sh` copies the redb database and (when present) the Tantivy
index into timestamped directories. It does **not** back up
`master.passphrase`, `keystore.json`, or `oauth_credentials.enc` unless those
files happen to be manually included by a separate procedure. The default
backup is therefore not a complete machine recovery set. Keep separately
protected copies of every required secret and state file, and test recovery
before relying on it.

The script is not an application-coordinated snapshot. Reflink support avoids
some live-copy races but does not guarantee that the database and search index
represent the same point in time; without reflink, copying the live database
may also race writes. Stop the service before making a recovery-grade backup.
Pruning removes completed marked snapshots older than the `--keep` count;
`--keep 0` disables pruning. It does not rotate secrets or make backups
recoverable. There is no automated restore or passphrase-rotation workflow.

If a keystore handle is missing, a subsequent key-creation call can generate a
new random key; data encrypted with the previous key remains unreadable. Restore
the original database/keystore and matching passphrase from a known-good backup
rather than deleting entries to force key creation.

### 4.2 Changing the master passphrase

Changing `CYBERMANJU_MASTER_PASSPHRASE` does not re-wrap existing keystore
handles. Calls with a different passphrase fail closed and preserve the old
handle; supply the original passphrase to continue accessing the existing key.
Do **not** delete a `keystore.json` entry or replace the handle with a newly
generated key: that does not rotate the encryption key and will strand all
blobs sealed by the old key. A supported in-place passphrase/key migration
procedure is not currently provided. Preserve the original passphrase and
back up the existing keystore and ciphertext before attempting any manual
migration. Sealed blobs are not automatically re-encrypted.

### 4.3 File encryption is not a complete recovery workflow

Native file/batch encryption uses the shared authenticated byte pipeline in
`crates/web/src/api/batch.rs`: ciphertext at `<path>.enc` and required metadata
at `<path>.enc.meta.json`. Both artifacts, the matching database key row, and
the original master passphrase are needed for recovery. After durable sidecar
installation and an authenticated read-back matching the source, the source
is captured by rename into a private sibling recovery directory and checked
again before removal. A replacement at the public pathname is never unlinked
by source retirement. Detected source changes retain all captured bytes, and
errors identify the recovery location. This is not secure erasure: filesystem snapshots, older file
versions, independent backups and previously uploaded copies may still contain
plaintext. A successful batch is a sequence of per-file transitions, not an
all-or-nothing transaction across all selected files.

**External in-place writers must be stopped during encrypt/decrypt.** Private
rename and byte verification do not revoke another process's already-open
writable handle. There is no portable atomic compare-bytes-and-unlink API in
this implementation; a write after the last comparison can still race removal.
The arbitrary concurrent-writer data-loss issue is therefore not fully closed.
Do not treat these changes as mandatory filesystem locking or a snapshot system.

Decrypt refuses tampered packages, missing keys and a conflicting existing
plaintext path. Interrupted transitions retain recovery sidecars; retry the
single-file decrypt operation rather than deleting artifacts or generating a
replacement key. Cleanup removes and syncs ciphertext before removing metadata,
so retries can validate plaintext if cleanup stops halfway. Unidentified orphan
ciphertext and malformed partial packages, when an unencrypted plaintext file is
present, are moved, not deleted, into private
recovery directories; the first call reports their location and a retry can
continue. Ambiguous packages with mismatching valid metadata still fail closed.
Sidecar installation uses exclusive creation rather than hard links. Its
portable copy mechanism is not atomic visibility for readers: partial final
files can exist after an interrupted write, and complete recovery stages are
retained on installation errors.

New sidecars and stages use owner-only permissions on Unix. New metadata stores
the source's ordinary Unix permission bits (including executable bits) and its
read-only state, which restoration and encryption-failure rollback preserve.
Setuid/setgid/sticky bits are deliberately not restored. Legacy metadata cannot
recover permissions it never recorded; its restored files default to `0600`.

The browser has a separate vault format. Its pipeline now rejects persistence
failures and attempts rollback, but multi-key vault writes are not a single
crash-atomic transaction. Browser private keys are currently stored unwrapped
beside ciphertext: possession of a complete vault copy permits decryption.
The native/server passphrase-sealing guarantees above do not apply to that
browser format, and these fixes do not add browser at-rest confidentiality.
Keep an independent known-good vault backup; these
changes do not provide a recovery artifact, recursive folder/trash workflow,
Shamir recovery or guaranteed cross-format interchange. Browser cipher tests
use real Node ChaCha20-Poly1305 behind a WASM shim; compiled Rust/WASM behavior
still requires CI validation.

---

## 5. Provider OAuth

Flow: authorization code + **PKCE (S256)** for Google (Drive / Photos),
GitHub and GitLab.

1. `GET /api/sync/oauth/{provider}/start?configId=<id>` → `{ authorizeUrl, state }`.
   Requires a session. Fails with `400` when the client id/secret env vars
   are missing.
2. The client opens `authorizeUrl` in a browser.
3. The provider redirects to
   `GET /api/sync/oauth/{provider}/callback?code=…&state=…` — **Public**, the
   `state` parameter is the credential (it is single-use and expires after
   10 minutes).
4. The code is redeemed server-side with the PKCE verifier; the resulting
   `OAuthCredentials` are sealed at rest under `<data dir>/oauth_credentials.enc`.

### Redirect URI registration

Providers require an **exact** match. The default is:

```
http://127.0.0.1:3456/api/sync/oauth/<provider>/callback
```

Register exactly that in the provider console for a local install. For a
reverse-proxied deployment, set:

```bash
CYBERMANJU_OAUTH_REDIRECT_URI=https://drive.example.com/api/sync/oauth/github/callback
```

and register *that* value instead.

### Client credentials

```bash
CYBERMANJU_OAUTH_GOOGLE_CLIENT_ID=…
CYBERMANJU_OAUTH_GOOGLE_CLIENT_SECRET=…
CYBERMANJU_OAUTH_GITHUB_CLIENT_ID=…
CYBERMANJU_OAUTH_GITHUB_CLIENT_SECRET=…
CYBERMANJU_OAUTH_GITLAB_CLIENT_ID=…
CYBERMANJU_OAUTH_GITLAB_CLIENT_SECRET=…
```

They are read from the environment only — never stored in the database, and
never serialized to a client.

### What a provider can see

* The ciphertext of the files you upload (AGENT-2 encrypts before upload).
* Your account id, upload timestamps, file sizes and object names — encrypt
  before upload *and* keep names opaque if that metadata matters.
* **Nothing** about other files, your identity inside the app, or your
  encryption keys: private keys are sealed at rest and are never sent.

---

## 6. HTTP hardening

Implemented in `crates/web/src/security.rs`, wired into both the embedded
server and `docker/server`:

* **Request line / header caps** — 8 KiB request line, 8 KiB per header,
  100 headers, 4 KiB `Authorization`.
* **Body cap** — 100 MB, rejected with `413` *before* the body is read.
* **Connection cap** — 256 concurrent connections, then `503`.
* **Rate limit** — 100 requests / 60 s per IP, `429` on overflow.
* **CORS** — an explicit allowlist. Arbitrary `Origin` headers are **never**
  reflected; unlisted origins get no `Access-Control-Allow-Origin` at all.
* **Security headers** on every response:
  `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`,
  `X-Frame-Options: DENY`, `Permissions-Policy: camera=(), microphone=(),
  geolocation=(), payment=()` and a CSP
  (`default-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`).
  HSTS is deliberately left to the TLS proxy.
* **Input validation** — usernames, display names, passwords, roles, ids and
  share tokens are validated centrally; `limit`/`offset` are clamped.

---

## 7. Environment variable reference

| Variable | Purpose |
|---|---|
| `CYBERMANJU_DATA_DIR` | Directory for every persisted secret |
| `CYBERMANJU_JWT_SECRET` | JWT signing secret (64 hex = used raw) |
| `CYBERMANJU_MASTER_PASSPHRASE` | Passphrase that unseals keys at rest |
| `CYBERMANJU_ADMIN_USERNAME` / `_PASSWORD` | Provision the first administrator |
| `CYBERMANJU_ALLOW_REGISTRATION` | `1` — allow registration once users exist |
| `CYBERMANJU_OAUTH_REDIRECT_URI` | Override the OAuth callback URI |
| `CYBERMANJU_OAUTH_{GOOGLE,GITHUB,GITLAB}_CLIENT_ID` / `_SECRET` | OAuth client credentials |
| `DB_PATH` | Location of the redb database (also locates the data dir) |

---

## 8. Reporting

Report suspected vulnerabilities via the repository's issue tracker with the
`security` label. Do not include live tokens, passphrases or private keys in
a public issue.
