# iOS runbook (P2-16 — landed: unsigned device IPA CI + configure script)

Native iOS target: CI ships an **unconditional unsigned arm64 device IPA**
on every run — both `ios-build` jobs (`ci.yml` + `release.yml`,
macos-latest) run `npm run tauri ios init` → `scripts/ios-configure.sh` →
`npx tauri ios build --ci --target aarch64 --no-sign`, then verify the IPA
stayed unsigned (no `embedded.mobileprovision`, Mach-O platform 2 = iOS
device, `codesign` reports the app bundle unsigned) and upload it as
`CyberManju-OS-<ver>-ios-device-unsigned.ipa`. The UI contract is pinned
(`tests/frontend/mobile-ios.test.ts` — springboard pager, safe-area,
squircle icons), the config floor is set (`bundle.ios.minimumSystemVersion`
in `src-tauri/tauri.conf.json`), and `scripts/ios-configure.sh` audits
version/floor/identifier/ATS.
`scripts/ios-signing.sh` (prepare/verify/cleanup) mirrors
`scripts/android-signing.sh`, but it is **manual/unused today**: no
workflow invokes it and there is no iOS `REQUIRE_SIGNING` anywhere. Users
sign the shipped IPA with their own Apple profile before installation; a
signed-when-secrets-exist flow is the remaining work (Status below), not
something this runbook pretends exists.

## 1. Scaffold the Xcode project

```sh
npx tauri ios init
```

This generates `src-tauri/gen/apple/` (gitignored, like Android's `gen/`).
Never commit it; CI regenerates per run.

## 2. Device build (macOS only)

What CI actually runs (`ios-build` in `ci.yml` + `release.yml`):

```sh
npx tauri ios init                # regenerate src-tauri/gen/apple (gitignored)
bash scripts/ios-configure.sh    # link SystemConfiguration + audits
npx tauri ios build --ci --target aarch64 --no-sign
```

The `--no-sign` device branch creates the arm64 device IPA directly from
the archive (do NOT add `--archive-only` — it suppresses IPA generation —
or `--export-method` — it invokes a distribution export such as App Store).
The job then asserts the IPA stayed unsigned (container has exactly one
`Payload/*.app` and no `embedded.mobileprovision`; binary is arm64 built
for platform 2, not the simulator's 7; `codesign -dv` must report "code
object is not signed at all") and renames it
`CyberManju-OS-<ver>-ios-device-unsigned.ipa` — **unsigned artifacts always
say `unsigned` in the file name** (the Android contract,
docs/ANDROID.md §2).

A signed build (ad-hoc / app-store / TestFlight) is manual today:
`scripts/ios-signing.sh prepare` / `verify` / `cleanup` exists and mirrors
`scripts/android-signing.sh` (ephemeral CI keychain, fail-closed `verify`
under `REQUIRE_SIGNING=1`, shred before upload), but no workflow wires it
in — there is no iOS `REQUIRE_SIGNING` in `ci.yml`/`release.yml`, so the
job never signs and never requires Apple secrets:

```sh
npx tauri ios build --export-method app-store # or ad-hoc / enterprise
# + scripts/ios-signing.sh prepare/verify/cleanup, secrets
#   APPLE_CERTIFICATE_P12_B64 / APPLE_CERTIFICATE_PASSWORD /
#   APPLE_PROVISIONING_PROFILE_B64 / KEYCHAIN_PASSWORD
```

## 3. Transport notes (same as Android)

- No dashboard on mobile: `MOBILE_NATIVE_OS_COMMANDS` serves the full cybsh
  OS layer + disks + devices + todos over IPC; REST is never probed without
  a server URL.
- LAN dashboard over plain HTTP (`http://nas:3456`) needs an App Transport
  Security exception: `NSAppTransportSecurity → NSAllowsLocalNetworking`
  (iOS 17+) or a per-domain `NSExceptionDomains` entry. HTTPS behind the
  Caddy sidecar (`Caddyfile` + `docker-compose.tls.yml`) needs no exception.
- OAuth callbacks use the `cybermanju://` deep link (already in the mobile
  deep-link config shared with Android).

## 4. Data protection parity (Android: `allowBackup=false` + manual export)

- iOS equivalent: exclude the vault from iCloud backup
  (`NSURLIsExcludedFromBackupKey` on the app-private files dir) and ship the
  same manual encrypted export path (`backup export` → Files app).
- Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`) for the
  device identity private half; never iCloud Keychain sync for vault keys.

## 5. Safari gaps (tested fallbacks)

- No File System Access API: Export/Import uses the Files-app fallback path
  (same branch Android exercises without FSA).
- No OPFS in older WebKit: localStorage volume mirror (same as Pages).

## Status

- [x] UI contract tests green
- [x] `bundle.ios` floor set
- [x] `scripts/ios-configure.sh` audits (version/floor/identifier/ATS) — wired into `ios-build` in `ci.yml` + `release.yml`
- [x] `ios-build` CI job on macos-latest: unsigned arm64 **device** IPA every run, `-ios-device-unsigned.ipa` naming, container + Mach-O platform + codesign assertions (no simulator job/artifact)
- [x] `tauri ios init` regenerates `gen/apple` per run (gitignored, like Android's `gen/`)
- [x] `scripts/ios-signing.sh` prepare/verify/cleanup written (ephemeral keychain) — manual/unused until a workflow wires it in
- [ ] Signed device build with real Apple secrets + TestFlight track (wire `ios-signing.sh` into `release.yml` first; needs `APPLE_CERTIFICATE_P12_B64` / `APPLE_PROVISIONING_PROFILE_B64` in repo secrets — sequenced, honest reason: Apple Developer account + signing, not code)
