# verify-grep-v2.ps1 - Verify v0.5.1 strings with correct messages
$spike = "F:\soft\00selfmade\filemanager\sigma-listary-spike\target\release\spike.exe"
$size = (Get-Item $spike).Length
Write-Host "spike.exe size: $size bytes" -ForegroundColor Cyan

# Read all bytes once
$allBytes = [System.IO.File]::ReadAllBytes($spike)
$content = [System.Text.Encoding]::ASCII.GetString($allBytes)

# Note: in Rust release builds, function names get mangled into symbols
# (e.g. _ZN4spikE4path_...). What matters for grep verify is whether
# the LOG STRINGS (which are not mangled) are present, plus the SPIKE_VERSION.
$strings = @(
    @{cat="MESSAGE"; n="v0.5.1 H1 success"; s="v0.5.1 H1 succeeded"},
    @{cat="MESSAGE"; n="v0.5.1 H1 failed"; s="v0.5.1 H1 failed"},
    @{cat="MESSAGE"; n="v0.5.1 H2 success"; s="v0.5.1 H2 succeeded"},
    @{cat="MESSAGE"; n="v0.5.1 H2 failed"; s="v0.5.1 H2 failed"},
    @{cat="MESSAGE"; n="v0.5.1 F3 fallback"; s="v0.5.1: H1 + H2 failed"},
    @{cat="MESSAGE"; n="WinUI 3 wrapped detected"; s="WinUI 3 wrapped detected"},
    @{cat="MESSAGE"; n="SendInput Ctrl+L dispatched"; s="SendInput Ctrl+L"},
    @{cat="MESSAGE"; n="foreground-lock blocked"; s="foreground-lock likely blocked"},
    @{cat="MESSAGE"; n="4-layer fallback chain"; s="4-layer fallback chain"},
    @{cat="MESSAGE"; n="AllowSetForegroundWindow"; s="AllowSetForegroundWindow"},
    @{cat="VERSION"; n="SPIKE_VERSION 0.5.1"; s="0.5.1"},
    @{cat="DEDUP_KIND"; n="set_path_dedup (event kind)"; s="set_path_dedup"},
    @{cat="DEDUP_MSG"; n="unchanged, skipping dialog write loop"; s="unchanged, skipping dialog write loop"},
    @{cat="ASFW_ANY"; n="ASFW_ANY (D1 fix marker)"; s="ASFW_ANY"},
    @{cat="DEAD_CODE"; n="D3 architecture comment"; s="D3 architectural"}
)

$pass = 0
$fail = 0
foreach ($item in $strings) {
    if ($content.Contains($item.s)) {
        Write-Host "  [$($item.cat)] PASS: $($item.n)" -ForegroundColor Green
        $pass++
    } else {
        Write-Host "  [$($item.cat)] FAIL: $($item.n)" -ForegroundColor Red
        $fail++
    }
}

Write-Host ""
Write-Host "Result: $pass passed, $fail failed" -ForegroundColor Cyan
Write-Host ""

# Sanity: the size should be larger than v0.5.0 spike.exe (no v0.5.0 binary to compare,
# but we know v0.5.0 spike.exe from handoff is in release folder)
$v050 = "F:\soft\00selfmade\filemanager\release\extension\bin\focus-sync-sidecar.exe"
if (Test-Path $v050) {
    $v050size = (Get-Item $v050).Length
    Write-Host "Compare with focus-sync-sidecar.exe (v0.5.0 shipped): $v050size bytes" -ForegroundColor Yellow
    Write-Host "  spike.exe (this v0.5.1 build): $size bytes" -ForegroundColor Cyan
    $delta = $size - $v050size
    Write-Host "  Delta: $delta bytes ($([math]::Round($delta/1024, 1)) KB)" -ForegroundColor Yellow
}