# baudelaire installer: a prebuilt, checksum-verified binary onto your path.
#
# The Windows counterpart of `install.sh`, and deliberately the same shape: the
# same knobs under the same names, the same steps in the same order, the same
# checksum promise. Only what genuinely differs is different — one libc, one
# architecture, a `.zip` around a `.exe`, and a `PATH` that no exported variable
# can change.
#
# ASCII only, markers included: Windows PowerShell 5.1 reads a BOM-less UTF-8
# script as the legacy code page, which turns any nicer glyph into mojibake on
# the one host most likely to run this.

[CmdletBinding()]
param(
  # Read from the environment rather than taken as arguments, because the
  # documented way to run this is `irm <url> | iex`, which can pass neither.
  # Naming them as parameters too keeps a downloaded copy scriptable.
  [string]$Repo    = $env:REPO,
  [string]$Api     = $env:API,
  [string]$Prefix  = $env:PREFIX,
  [string]$Version = $env:VERSION,
  # full, or slim: only system fonts + assets copied as-is
  [string]$Flavor  = $env:FLAVOR
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $Repo)   { $Repo   = 'https://github.com/cestef/baudelaire' }
# Release metadata lives on a different host than the release downloads, so the
# API base is its own knob rather than a suffix on $Repo.
if (-not $Api)    { $Api    = 'https://api.github.com/repos/cestef/baudelaire' }
if (-not $Prefix) { $Prefix = Join-Path $env:LOCALAPPDATA 'Programs\baudelaire' }
if (-not $Flavor) { $Flavor = 'full' }

function Step($message) {
  Write-Host '-> ' -ForegroundColor Magenta -NoNewline
  Write-Host $message
}
function Die($message) {
  Write-Host 'error: ' -ForegroundColor Red -NoNewline
  Write-Host $message
  exit 1
}

$src = "build from source: cargo install --git $Repo"

# Windows PowerShell 5.1 still negotiates TLS 1.0 by default, which GitHub
# refuses. Harmless on PowerShell 7, where the OS picks the protocol.
try {
  [Net.ServicePointManager]::SecurityProtocol =
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch { }

# One prebuilt Windows target. ARM64 does run the x86_64 build under emulation,
# but slowly and with nothing here able to say so, so it is refused by name
# rather than installed as a surprise.
$native = "$env:PROCESSOR_ARCHITECTURE $env:PROCESSOR_ARCHITEW6432"
if ($native -notlike '*AMD64*') {
  Die "no prebuilt binary for $env:PROCESSOR_ARCHITECTURE. $src"
}
$arch = 'x86_64'

$suffix = switch ($Flavor) {
  'full'  { '' }
  'slim'  { '-slim' }
  default { Die "unknown FLAVOR=$Flavor, use full or slim" }
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('baudelaire-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
$target = Join-Path $Prefix 'baudelaire.exe'
try {
  # latest tag
  if (-not $Version) {
    Step 'resolving latest release'
    try {
      $release = Invoke-RestMethod -UseBasicParsing -Uri "$Api/releases/latest"
    } catch { Die 'release lookup failed' }
    # Through `Select-Object` so a response without the field is the message
    # below rather than a strict-mode property error.
    $Version = $release | Select-Object -ExpandProperty tag_name -ErrorAction SilentlyContinue
    if (-not $Version) { Die 'couldn''t resolve latest tag, pin one with VERSION=' }
  }

  # download zip + sha
  $asset = "baudelaire-windows-$arch$suffix.zip"
  Step "downloading baudelaire $Version (windows-$arch, $Flavor)"
  foreach ($file in @($asset, "$asset.sha256")) {
    try {
      Invoke-WebRequest -UseBasicParsing `
        -Uri "$Repo/releases/download/$Version/$file" `
        -OutFile (Join-Path $tmp $file)
    } catch { Die "download failed: $file" }
  }

  # verify + install
  # The checksum is fetched from the same origin as the archive, so it proves
  # the transfer was not truncated or corrupted; it is not a signature and does
  # not prove authenticity.
  Step 'verifying checksum'
  # `sha256sum` output: the digest, whitespace, then the file it covers.
  $expected = ((Get-Content -Raw (Join-Path $tmp "$asset.sha256")).Trim() -split '\s+')[0]
  $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $tmp $asset)).Hash
  if ($actual -ne $expected.ToUpperInvariant()) { Die 'checksum mismatch, aborting' }

  Expand-Archive -Path (Join-Path $tmp $asset) -DestinationPath $tmp -Force
  New-Item -ItemType Directory -Path $Prefix -Force | Out-Null
  try {
    Copy-Item -Path (Join-Path $tmp 'baudelaire.exe') -Destination $target -Force
  } catch { Die "install to $Prefix failed" }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host 'done: ' -ForegroundColor Green -NoNewline
Write-Host "installed baudelaire $Version -> $target"

# `PATH` here is this process's copy, so the hint is the command that actually
# persists one, and it is printed rather than run: an installer that rewrites
# the user environment unasked is a surprise, and this one mirrors
# `install.sh`, which only ever prints.
$listed = $env:PATH -split ';' | Where-Object { $_.TrimEnd('\') -eq $Prefix.TrimEnd('\') }
if ($listed) {
  Write-Host '  baudelaire init to scaffold a site'
} else {
  Write-Host '  not on PATH, add it with:'
  Write-Host ("  [Environment]::SetEnvironmentVariable('Path', " +
    "[Environment]::GetEnvironmentVariable('Path','User') + ';$Prefix', 'User')")
}
