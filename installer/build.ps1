# Builds the release exe, then the Inno Setup installer (if ISCC is found) and a portable zip.
#   -SkipCargo        reuse the existing targetelease\snowy.exe (CI signs it before packaging)
#   -RequireInstaller fail instead of warning when Inno Setup 6 is missing
param(
  [switch]$SkipCargo,
  [switch]$RequireInstaller
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $SkipCargo) {
  cargo build -p snowy --release
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} elseif (-not (Test-Path 'targetelease\snowy.exe')) {
  throw "-SkipCargo given but targetelease\snowy.exe does not exist"
}

$version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null

$iscc = @(
  "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
  "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
  "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($iscc) {
  & $iscc "/DMyAppVersion=$version" (Join-Path $root 'installer\snowy.iss')
  if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
} elseif ($RequireInstaller) {
  throw "Inno Setup 6 not found (ISCC.exe)"
} else {
  Write-Warning "Inno Setup 6 not found; skipping installer. Install it with: winget install JRSoftware.InnoSetup"
}

$zip = Join-Path $dist "Snowy-$version-portable.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path 'target\release\snowy.exe', 'README.md' -DestinationPath $zip
Write-Host ("Portable zip: {0} ({1} KB)" -f $zip, [math]::Round((Get-Item $zip).Length / 1KB))
