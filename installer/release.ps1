# Builds the release (via build.ps1), writes the SHA-256 checksum next to the installer and
# prints the command that publishes the GitHub release. Pass -Publish to actually run it.
#   powershell -ExecutionPolicy Bypass -File installer\release.ps1 [-Notes notes.md] [-Publish]
param(
  [string]$Notes,
  [switch]$Publish
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw "could not read the version from Cargo.toml" }

& (Join-Path $PSScriptRoot 'build.ps1')
if ($LASTEXITCODE -ne 0) { throw "build.ps1 failed" }

$setupName = "Snowy-$version-setup.exe"
$setup = Join-Path $root "dist\$setupName"
$zip = Join-Path $root "dist\Snowy-$version-portable.zip"
foreach ($f in @($setup, $zip)) {
  if (-not (Test-Path $f)) { throw "missing $f (is Inno Setup 6 installed?)" }
}

# "<lowercase hex>  <file name>", the format `sha256sum -c` understands; the app reads the first token.
$hash = (Get-FileHash -Algorithm SHA256 -Path $setup).Hash.ToLowerInvariant()
$shaFile = "$setup.sha256"
[System.IO.File]::WriteAllText($shaFile, "$hash  $setupName`n", (New-Object System.Text.UTF8Encoding($false)))
Write-Host "SHA-256: $hash"
Write-Host "Wrote $shaFile"

if (-not $Notes) { $Notes = Join-Path $root "dist\release-notes-$version.md" }
if (-not (Test-Path $Notes)) { Write-Warning "Notes file not found: $Notes (create it before publishing)" }

$cmd = "gh release create v$version dist\$setupName dist\$setupName.sha256 dist\Snowy-$version-portable.zip --repo berkcantmr/snowy --title `"Snowy $version`" --notes-file `"$Notes`""
if ($Publish) {
  if (-not (Test-Path $Notes)) { throw "notes file not found: $Notes" }
  Write-Host "Publishing: $cmd"
  gh release create "v$version" "dist\$setupName" "dist\$setupName.sha256" "dist\Snowy-$version-portable.zip" --repo berkcantmr/snowy --title "Snowy $version" --notes-file $Notes
  if ($LASTEXITCODE -ne 0) { throw "gh release create failed" }
} else {
  Write-Host ""
  Write-Host "To publish, run:"
  Write-Host "  $cmd"
}
