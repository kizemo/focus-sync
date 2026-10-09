# ============================================================================
# build.ps1 — Compile kizemo.focus-sync-0.2.0-setup.exe via NSIS
# ============================================================================
# Run from this directory. Output:
#   kizemo.focus-sync-0.2.0-setup.exe  (in this directory)
# Also copied to ../ (release/ top-level) for visibility.
# ============================================================================

$ErrorActionPreference = 'Stop'

$nsisDir = 'C:\Program Files (x86)\NSIS'
$makensis = Join-Path $nsisDir 'makensis.exe'
$installerNsi = Join-Path $PSScriptRoot 'installer.nsi'
$outExe = Join-Path $PSScriptRoot 'kizemo.focus-sync-0.5.8-setup.exe'

if (-not (Test-Path $makensis)) {
    Write-Error "makensis.exe not found at $makensis. Adjust `$nsisDir in build.ps1."
    exit 1
}

if (-not (Test-Path $installerNsi)) {
    Write-Error "installer.nsi not found at $installerNsi. Run from extension-installer dir."
    exit 1
}

Write-Host "==> Compiling installer..."
& $makensis /V2 $installerNsi
if ($LASTEXITCODE -ne 0) {
    Write-Error "makensis failed (exit $LASTEXITCODE). See error output above."
    exit $LASTEXITCODE
}

if (-not (Test-Path $outExe)) {
    Write-Error "Expected output not found: $outExe"
    exit 1
}

$size = (Get-Item $outExe).Length
Write-Host "==> Built: $outExe ($size bytes)"

# Also copy to release/ top-level for easy access
$topLevel = Join-Path (Join-Path $PSScriptRoot '..') 'kizemo.focus-sync-0.5.8-setup.exe'
Copy-Item $outExe $topLevel -Force
Write-Host "==> Copied to: $topLevel"
exit 0