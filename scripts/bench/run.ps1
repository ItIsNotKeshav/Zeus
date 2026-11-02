# Runs the Zeus benchmark a few times on Windows and writes one JSON
# file per run under bench-results\<label>\.
#
#   powershell -ExecutionPolicy Bypass -File scripts\bench\run.ps1 [-Binary PATH] [-Label NAME] [-Runs N] [-Out DIR]
#
# The binary defaults to the zeus.exe beside this folder, as in a bench
# download; the label to the one in BUILD_INFO.txt beside it. Each run takes
# about 2 minutes; the default two, with the pause between them, about 5.
param(
    [string]$Binary = "",
    [string]$Label = "",
    [int]$Runs = 2,
    [string]$Out = ""
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "lib.ps1")

$Binary = Resolve-BenchBinary $Binary
if (-not $Label) { $Label = Get-BuildInfo $Binary "label" }
if (-not $Label) { Write-Host "Name this build, for example -Label main"; exit 2 }
if ($Label -notmatch '^[A-Za-z0-9._+-]+$') { Write-Host "Use letters, digits and . _ + - in the label"; exit 2 }
if (-not $Out) { $Out = Join-Path (Get-Location) "bench-results" }
$folder = Join-Path $Out $Label
New-Item -ItemType Directory -Force -Path $folder | Out-Null

$shell = (Get-Process -Id $PID).Path
$failed = 0
for ($run = 1; $run -le $Runs; $run++) {
    $file = Join-Path $folder ("run-{0:D2}.json" -f $run)
    # The same PowerShell that runs this, Windows PowerShell or PowerShell 7.
    & $shell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "sample.ps1") `
        -Binary $Binary -Out $file -Label $Label -Run $run
    $code = $LASTEXITCODE
    # A build that cannot run the benchmark fails the same way every time.
    if (($code -eq 2 -or $code -eq 3) -and -not (Test-Path $file)) { exit $code }
    if ($code -ne 0) { $failed++ }
    # Let the machine settle between runs.
    if ($run -lt $Runs) { Start-Sleep -Seconds 5 }
}

Write-Host ""
Write-Host "Done: $Runs runs of $Label in $folder ($failed failed)."
Write-Host "Send back that folder (zip it). To compare builds:"
Write-Host "  python scripts\bench\compare.py $Out\*"
if ($failed) { exit 1 }
