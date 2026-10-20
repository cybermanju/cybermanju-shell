// CyberManju OS — install links (pure URL builders + release resolver).
// Role: single source of truth for every "get the native app" surface:
//   deterministic links (installer scripts, GHCR, AUR, release page) plus a
//   live GitHub-release asset resolver so per-file download buttons stay
//   direct without hardcoding versioned bundle names.
// Exports: `INSTALL_REPO`, `INSTALL_TAG_URLS`, `INSTALL_TARGETS`,
//   `fetchLatestReleaseAssets`, `pickInstallAssetUrls`, `requestInstallWindow`,
//   `isInstallNeededError` (native-build remedy predicate).
// Integrates: `InstallPanel.vue` renders the table; `App.vue` opens the
//   window on the `cybermanju:open-install` event this module fires.
// Contracts: `unsupported:` never appears here — unknown assets fall back
//   to the release page; every failure is a graceful fallback, never a throw
//   past the panel (the panel shows the fallback link instead).

/** Release owner/repo — mirrors `install.sh`/`install.ps1` defaults. */
export const INSTALL_REPO = 'cybermanju/cybermanju.github.io'

export const releasePageUrl = (tag = 'latest'): string =>
  `https://github.com/${INSTALL_REPO}/releases/${tag === 'latest' ? 'latest' : `tag/${tag}`}`

/** Direct per-file redirect (GitHub resolves `latest` → the newest tag). */
export const releaseAssetUrl = (asset: string, tag = 'latest'): string =>
  tag === 'latest'
    ? `https://github.com/${INSTALL_REPO}/releases/latest/download/${asset}`
    : `https://github.com/${INSTALL_REPO}/releases/download/${tag}/${asset}`

export const INSTALL_TAG_URLS = {
  installSh: 'https://cybermanju.github.io/install.sh',
  installPs1: 'https://cybermanju.github.io/install.ps1',
  /** Tagged source the AUR build clones (`aur/PKGBUILD`). */
  aurGit: `https://github.com/${INSTALL_REPO}.git`,
  aurTree: `https://github.com/${INSTALL_REPO}/tree/main/aur`,
  ghcrImage: 'ghcr.io/cybermanju/cybermanju-os',
  composeFile: `https://github.com/${INSTALL_REPO}/blob/main/docker-compose.yml`,
  sha256: (tag = 'latest'): string => releaseAssetUrl('SHA256SUMS.txt', tag),
}

export type InstallLogo = 'android' | 'ubuntu' | 'arch' | 'windows' | 'macos' | 'docker'

export interface InstallLink {
  label: string
  /** Direct URL, or '' when resolved live from the release (see `assetKey`). */
  href: string
  /** Release-asset matcher used by `pickInstallAssetUrls` (`ext:`/`has:`). */
  assetKey?: string
  note?: string
}

export interface InstallTarget {
  id: string
  logo: InstallLogo
  os: string
  title: string
  blurb: string
  /** GUI / store / package downloads. */
  app: InstallLink[]
  /** Terminal (`cyb`) downloads + commands. */
  cli: InstallLink[]
  /** Per-OS specifics: package names, deps, commands, gotchas. */
  specifics: string[]
}

/**
 * App × CLI table. `href: ''` + `assetKey` rows resolve live against the
 * newest release (bundle names carry versions, so they cannot be
 * hardcoded); everything else links directly.
 */
export const INSTALL_TARGETS: InstallTarget[] = [
  {
    id: 'android',
    logo: 'android',
    os: 'Android',
    title: 'Android app',
    blurb: 'Native WebView build (arm64-v8a, signed). Device files browse straight from the sdcard — no picker needed.',
    app: [
      { label: 'APK (install on device)', href: '', assetKey: 'ext:.apk', note: 'arm64-v8a · allow unknown sources once' },
      { label: 'AAB (Play upload)', href: '', assetKey: 'ext:.aab', note: 'split-per-abi release bundle' },
    ],
    cli: [],
    specifics: [
      'Needs Android 7.0+ (minSdk 24); the APK is signed — verify against SHA256SUMS.',
      'Deep links use the cybermanju:// scheme (same as desktop).',
      'No server needed: the app ships its own redb database.',
    ],
  },
  {
    id: 'ubuntu',
    logo: 'ubuntu',
    os: 'Ubuntu / Debian',
    title: 'Ubuntu app (.deb)',
    blurb: 'Desktop Tauri build with full host-filesystem access (`/host` browses $HOME directly).',
    app: [
      { label: '.deb package', href: '', assetKey: 'ext:.deb', note: 'sudo dpkg -i' },
      { label: 'AppImage (portable)', href: '', assetKey: 'ext:.AppImage', note: 'chmod +x, then run' },
      { label: '.rpm (Fedora/openSUSE)', href: '', assetKey: 'ext:.rpm', note: 'sudo rpm -i / dnf install' },
      { label: 'Flatpak', href: '', assetKey: 'ext:.flatpak', note: 'id com.cybermanju.os' },
    ],
    cli: [
      { label: 'cyb .deb (amd64/arm64)', href: '', assetKey: 're:cyb_.*\\.deb$', note: 'the terminal companion' },
      { label: 'One-line install', href: INSTALL_TAG_URLS.installSh, note: 'curl -fsSL https://cybermanju.github.io/install.sh | bash' },
    ],
    specifics: [
      'System deps: libwebkit2gtk-4.1-0 + libgtk-3-0 (the .deb declares them).',
      'On Wayland/VMs with a black window: WEBKIT_DISABLE_DMABUF_RENDERER=1 ./CyberManju*.AppImage.',
      'Software GL fallback: LIBGL_ALWAYS_SOFTWARE=1; force X11 with GDK_BACKEND=x11.',
    ],
  },
  {
    id: 'arch',
    logo: 'arch',
    os: 'Arch / CachyOS',
    title: 'Arch app (AUR)',
    blurb: 'Built from the tagged source via the AUR package (same Tauri bundle as Ubuntu).',
    app: [
      { label: 'AUR package (cybermanju-os)', href: INSTALL_TAG_URLS.aurTree, note: 'cd aur/ && makepkg -si' },
      { label: 'AppImage (portable)', href: '', assetKey: 'ext:.AppImage', note: 'chmod +x, then run' },
    ],
    cli: [
      { label: 'One-line install', href: INSTALL_TAG_URLS.installSh, note: 'curl -fsSL https://cybermanju.github.io/install.sh | bash' },
      { label: 'cyb tarball (linux amd64/arm64)', href: '', assetKey: 're:cyb-linux-.*\\.tar\\.gz$', note: 'extracts a cyb binary' },
    ],
    specifics: [
      'Arch/CachyOS need NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1 (linuxdeploy cannot read .relr.dyn or mount FUSE in containers).',
      'Build deps: cargo · nodejs · npm · pkg-config · base-devel (+ webkit2gtk-4.1, gtk3, libsoup3, openssl).',
      'The AUR source pins the release tag (sha256sums SKIP is correct for a VCS source).',
    ],
  },
  {
    id: 'windows',
    logo: 'windows',
    os: 'Windows',
    title: 'Windows app',
    blurb: 'Desktop Tauri build (MSI + NSIS .exe). WebView2 is pre-installed on Windows 10/11.',
    app: [
      { label: 'Installer (.msi)', href: '', assetKey: 'ext:.msi', note: 'system-wide install' },
      { label: 'Setup (.exe, NSIS)', href: '', assetKey: 're:.*(_setup|\\.exe)$', note: 'per-user install' },
    ],
    cli: [
      { label: 'One-line install (PowerShell)', href: INSTALL_TAG_URLS.installPs1, note: 'irm https://cybermanju.github.io/install.ps1 | iex' },
      { label: 'cyb tarball (windows amd64/arm64)', href: '', assetKey: 're:cyb-windows-.*\\.tar\\.gz$', note: 'carries cyb.exe' },
    ],
    specifics: [
      'Windows 10/11 ship WebView2 already — no extra runtime to install.',
      'The CLI lands in %LOCALAPPDATA%\\cybermanju\\bin (added to the user PATH).',
      'Git-Bash/MSYS2/WSL can also run the universal install.sh.',
    ],
  },
  {
    id: 'macos',
    logo: 'macos',
    os: 'macOS',
    title: 'macOS app',
    blurb: 'Desktop Tauri build (.dmg) with the same vault + device-files support.',
    app: [
      { label: 'Disk image (.dmg)', href: '', assetKey: 'ext:.dmg', note: 'drag into /Applications' },
    ],
    cli: [
      { label: 'One-line install', href: INSTALL_TAG_URLS.installSh, note: 'curl -fsSL https://cybermanju.github.io/install.sh | bash' },
      { label: 'cyb tarball (macOS amd64/arm64)', href: '', assetKey: 're:cyb-darwin-.*\\.tar\\.gz$', note: 'extracts a cyb binary' },
    ],
    specifics: [
      'Checksum-verify with shasum -a 256 against the release SHA256SUMS.',
      'First launch may need a right-click → Open (unsigned developer warning).',
    ],
  },
  {
    id: 'docker',
    logo: 'docker',
    os: 'Docker / NAS',
    title: 'Self-host (Docker)',
    blurb: 'Headless dashboard on :3456 for any LAN device — the web UI then browses the server host.',
    app: [
      { label: 'GHCR image', href: 'https://github.com/cybermanju/cybermanju.github.io/pkgs/container/cybermanju-os', note: 'ghcr.io/cybermanju/cybermanju-os:latest' },
      { label: 'compose file', href: INSTALL_TAG_URLS.composeFile, note: 'docker compose up -d → http://<nas-ip>:3456' },
    ],
    cli: [
      { label: 'Serve via cyb', href: INSTALL_TAG_URLS.installSh, note: 'cyb serve up (Docker under the hood)' },
    ],
    specifics: [
      'Hardened compose: read_only · cap_drop ALL · no-new-privileges · mem_limit 1g · LAN-only port.',
      'Persist CYBERMANJU_JWT_SECRET (0600) + /data/cybermanju.db; healthcheck hits /api/readyz.',
      'ZimaOS metadata (x-casaos, amd64+arm64) is baked into docker-compose.yml.',
    ],
  },
]

export interface ReleaseAsset {
  name: string
  url: string
}

/** Raw asset list of the newest GitHub release (throws on network failure). */
export async function fetchLatestReleaseAssets(): Promise<{ tag: string; assets: ReleaseAsset[] }> {
  const res = await fetch(`https://api.github.com/repos/${INSTALL_REPO}/releases/latest`, {
    headers: { Accept: 'application/vnd.github+json' },
  })
  if (!res.ok) throw new Error(`release lookup failed: HTTP ${res.status}`)
  const body = (await res.json()) as {
    tag_name?: string
    assets?: Array<{ name?: string; browser_download_url?: string }>
  }
  const tag = String(body.tag_name ?? 'latest')
  const assets: ReleaseAsset[] = (Array.isArray(body.assets) ? body.assets : [])
    .filter((a) => typeof a?.name === 'string' && typeof a?.browser_download_url === 'string')
    .map((a) => ({ name: String(a.name), url: String(a.browser_download_url) }))
  return { tag, assets }
}

function matchAssetKey(name: string, key: string): boolean {
  if (key.startsWith('ext:')) return name.toLowerCase().endsWith(key.slice(4).toLowerCase())
  if (key.startsWith('re:')) {
    try {
      return new RegExp(key.slice(3), 'i').test(name)
    } catch {
      return false
    }
  }
  if (key.startsWith('has:')) return name.toLowerCase().includes(key.slice(4).toLowerCase())
  return name === key
}

/**
 * Resolve every `assetKey` row to a direct download URL. Unmatched rows
 * fall back to the release page (never a dead button). Pure — unit-tested.
 */
export function pickInstallAssetUrls(
  assets: ReleaseAsset[],
  tag = 'latest',
): Record<string, string> {
  const out: Record<string, string> = {}
  for (const target of INSTALL_TARGETS) {
    for (const link of [...target.app, ...target.cli]) {
      if (!link.assetKey) continue
      const hit = assets.find((a) => matchAssetKey(a.name, link.assetKey as string))
      out[`${target.id}:${link.label}`] = hit ? hit.url : releasePageUrl(tag)
    }
  }
  return out
}

// ── open-install event (unsupported → Install window) ───────────────────
// The store cannot import the window manager (cycle through
// WindowContent), so unsupported paths fire this event and `App.vue`
// opens the Install window from it — one path for every caller.

export const INSTALL_EVENT = 'cybermanju:open-install'

/** Ask the shell to open the Install window (center-screen, with `reason`). */
export function requestInstallWindow(reason = 'host-unsupported'): void {
  try {
    window.dispatchEvent(new CustomEvent(INSTALL_EVENT, { detail: { reason } }))
  } catch {
    // Non-browser runtimes (tests/SSR) have no window to open.
  }
}

/**
 * True when an error means "this needs a native build" — the Install
 * window is the remedy (desktop/Android app, Docker image). Deliberately
 * narrow: provider-shaped `unsupported:` (bad repo name, bad Drive path,
 * unknown compress layer, locked vault, stale wasm bundle) must NOT match
 * — their fix is not installing anything. Pure — unit-tested.
 */
export function isInstallNeededError(text: string): boolean {
  const t = String(text ?? '')
  if (!t) return false
  return (
    /no (folder picker|file system access api)/i.test(t) ||
    /(requires?|needs?) the (tauri desktop|desktop|native( android( or ios)?| ios)?) (app|build)/i.test(t) ||
    /needs? the dashboard/i.test(t) ||
    /open this \S+ in the (desktop app|android app)/i.test(t) ||
    /run it in the (desktop app|native android build)/i.test(t) ||
    /push from the desktop app/i.test(t)
  )
}

/**
 * Panel reason for an install-needed error: picker-shaped wordings get the
 * device-files explanation, everything else the general native-build one.
 * Pure — unit-tested.
 */
export function installReasonFor(text: string): string {
  const t = String(text ?? '')
  return /no (folder picker|file system access api)/i.test(t) ? 'host-unsupported' : 'needs-app'
}
