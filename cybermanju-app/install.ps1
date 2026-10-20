# CyberManju OS — universal `cyb` installer for native Windows PowerShell.
#   irm https://cybermanju.github.io/install.ps1 | iex
# Checksum-verified against the release SHA256SUMS.
#
# Env knobs: $env:CYB_VERSION (release tag, default latest),
#            $env:INSTALL_DIR (default $env:LOCALAPPDATA\cybermanju\bin)
# Onboarding (backend + setup walkthrough, prompted when interactive):
#            $env:CYB_SETUP (1 run / 0 skip / auto prompt, default auto),
#            $env:CYB_BACKEND (remote|docker|native|supabase),
#            $env:CYB_SERVE (1 = shorthand for docker),
#            $env:CYB_SERVER, $env:CYB_PORT,
#            $env:CYB_USERNAME / $env:CYB_PASSWORD (unattended setup),
#            $env:CYB_SUPA_URL / $env:CYB_SUPA_KEY (supabase broker pair).
$ErrorActionPreference = 'Stop'

$Repo = if ($env:CYB_REPO) { $env:CYB_REPO } else { 'cybermanju/cybermanju.github.io' }
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'cybermanju\bin' }
$WantVersion = if ($env:CYB_VERSION) { $env:CYB_VERSION } else { 'latest' }

if ([Net.ServicePointManager]::SecurityProtocol -notmatch 'Tls12') {
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

if ($WantVersion -eq 'latest') {
  Write-Host '◈ resolving latest release…'
  $rel = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
  $Tag = $rel.tag_name
  if (-not $Tag) { throw 'could not resolve the latest release' }
} else {
  $Tag = $WantVersion
}

$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'amd64' }
$Asset = "cyb-windows-$Arch.tar.gz"
$Base = "https://github.com/$Repo/releases/download/$Tag"
$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("cyb-install-" + [Guid]::NewGuid().ToString('N')))
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
  Write-Host "◈ installing cyb $Tag (windows/$Arch)…"
  Invoke-WebRequest "$Base/$Asset" -OutFile (Join-Path $Tmp $Asset)
  Invoke-WebRequest "$Base/SHA256SUMS.txt" -OutFile (Join-Path $Tmp 'SHA256SUMS.txt')

  $wantLine = Select-String -Path (Join-Path $Tmp 'SHA256SUMS.txt') -Pattern ([regex]::Escape($Asset)) | Select-Object -First 1
  if (-not $wantLine) { throw "no checksum entry for $Asset" }
  $wantSum = ($wantLine.Line -split '\s+')[0]
  $gotSum = (Get-FileHash (Join-Path $Tmp $Asset) -Algorithm SHA256).Hash.ToLower()
  if ($gotSum -ne $wantSum.ToLower()) { throw "checksum mismatch for $Asset (integrity: refusing to install)" }
  Write-Host '◈ checksum verified'

  # tar ships inbox on Windows 10+ and handles .tar.gz.
  tar -xzf (Join-Path $Tmp $Asset) -C $Tmp
  $bin = Join-Path $Tmp 'cyb.exe'
  if (-not (Test-Path $bin)) { throw 'tarball contains no cyb.exe' }

  New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
  Copy-Item $bin (Join-Path $InstallDir 'cyb.exe') -Force

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if ($userPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$InstallDir", 'User')
    Write-Host "→ added to user PATH (restart the terminal): $InstallDir"
  }
  Write-Host "◈ installed: $(Join-Path $InstallDir 'cyb.exe')"
} finally {
  Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

# --- optional onboarding: backend + setup -------------------------------
# Env-driven (`iex` cannot take script args): prompts only when interactive.
# Best-effort from here on: never let a failed probe abort the installer.
$ErrorActionPreference = 'Continue'
$cybExe = Join-Path $InstallDir 'cyb.exe'
if ($env:Path -notlike "*$InstallDir*") { $env:Path = "$InstallDir;$env:Path" }
$setupPref = if ($env:CYB_SETUP) { $env:CYB_SETUP } else { 'auto' }
$backend = $env:CYB_BACKEND
if ((-not $backend) -and ($env:CYB_SERVE -eq '1')) { $backend = 'docker' }
$interactive = [Environment]::UserInteractive

if ($backend -eq 'docker') {
  $upArgs = @('serve', 'up')
  if ($env:CYB_PORT) { $upArgs += '--port', $env:CYB_PORT }
  Write-Host '◈ starting local dashboard (cyb serve up)…'
  & $cybExe @upArgs
  if ($LASTEXITCODE -ne 0) { Write-Warning 'serve up failed — retry with: cyb serve up' }
} elseif ($backend -eq 'native') {
  $nativeArgs = @('backend', 'use', 'native')
  if ($env:CYB_PORT) { $nativeArgs += '--port', $env:CYB_PORT }
  Write-Host '◈ selecting native-app backend…'
  & $cybExe @nativeArgs
  if ($LASTEXITCODE -ne 0) { Write-Warning 'native backend needs the app running — retry with: cyb backend use native' }
} elseif ($backend -eq 'remote') {
  $srv = $env:CYB_SERVER
  if (-not $srv -and $interactive) {
    try { $srv = Read-Host '◈ Dashboard URL [http://127.0.0.1:3456]' } catch { $srv = '' }
    if (-not $srv) { $srv = 'http://127.0.0.1:3456' }
  }
  if (-not $srv) { Write-Warning 'remote backend needs $env:CYB_SERVER — skipped (cyb backend use remote --server URL)' }
  else { & $cybExe backend use remote --server $srv }
} elseif ($backend -eq 'supabase') {
  $supaUrl = $env:CYB_SUPA_URL
  $supaKey = $env:CYB_SUPA_KEY
  if ((-not $supaUrl -or -not $supaKey) -and $interactive) {
    try {
      if (-not $supaUrl) { $supaUrl = Read-Host '◈ Supabase project URL' }
      if (-not $supaKey) {
        $sec = Read-Host '◈ Supabase anon key' -AsSecureString
        $supaKey = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
          [Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
      }
    } catch { }
  }
  if (-not $supaUrl -or -not $supaKey) { Write-Warning 'supabase needs $env:CYB_SUPA_URL + $env:CYB_SUPA_KEY — skipped' }
  else { & $cybExe backend use supabase --url $supaUrl --key $supaKey }
} elseif ($backend) {
  Write-Warning "unknown CYB_BACKEND '$backend' (want remote|docker|native|supabase)"
} elseif ($interactive -and $setupPref -eq 'auto') {
  Write-Host '◈ Where should cyb connect? (backend)'
  Write-Host '  1) docker    local dashboard container — recommended (cyb serve up)'
  Write-Host '  2) native    installed CyberManju OS app dashboard (localhost)'
  Write-Host '  3) remote    dashboard URL you already run'
  Write-Host '  4) supabase  broker-direct, read-only (limited mode)'
  Write-Host '  5) skip      configure later with `cyb backend use`'
  try { $pick = Read-Host '◈ backend [1]' } catch { $pick = '' }
  if (-not $pick) { $pick = '1' }
  switch ($pick) {
    { $_ -in '1', 'docker' } {
      Write-Host '◈ starting local dashboard (cyb serve up)…'
      & $cybExe serve up
      if ($LASTEXITCODE -ne 0) { Write-Warning 'serve up failed — retry with: cyb serve up' }
    }
    { $_ -in '2', 'native' } { & $cybExe backend use native }
    { $_ -in '3', 'remote' } {
      try { $srv = Read-Host '◈ Dashboard URL [http://127.0.0.1:3456]' } catch { $srv = '' }
      if (-not $srv) { $srv = 'http://127.0.0.1:3456' }
      & $cybExe backend use remote --server $srv
    }
    { $_ -in '4', 'supabase' } {
      try {
        $supaUrl = Read-Host '◈ Supabase project URL'
        $sec = Read-Host '◈ Supabase anon key' -AsSecureString
        $supaKey = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
          [Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
        & $cybExe backend use supabase --url $supaUrl --key $supaKey
      } catch { Write-Warning 'supabase setup skipped' }
    }
  }
}

$wantSetup = $false
switch ($setupPref) {
  { $_ -in '1', 'yes', 'true', 'always' } { $wantSetup = $true }
  { $_ -in '0', 'no', 'false', 'never', 'skip' } { $wantSetup = $false }
  default {
    if ($env:CYB_USERNAME -and $env:CYB_PASSWORD) { $wantSetup = $true }
    elseif ($interactive) {
      try { $ans = Read-Host '◈ Run the setup walkthrough now (cyb setup)? [Y/n]' }
      catch { $ans = 'n' }
      $wantSetup = ($ans -eq '' -or $ans -match '^[Yy]')
    }
  }
}

if ($wantSetup) {
  if ($env:CYB_USERNAME -and $env:CYB_PASSWORD) {
    Write-Host '◈ running cyb setup (non-interactive)…'
    $setupArgs = @('setup', '--username', $env:CYB_USERNAME, '--password', $env:CYB_PASSWORD)
    if ($env:CYB_SERVER) { $setupArgs += '--server', $env:CYB_SERVER }
    & $cybExe @setupArgs
  } elseif ($env:CYB_SERVER) { & $cybExe setup --server $env:CYB_SERVER } else { & $cybExe setup }
  if ($LASTEXITCODE -ne 0) { Write-Warning 'setup exited — re-run `cyb setup`.' }
} else {
  Write-Host '→ start here:  cyb setup'
}
