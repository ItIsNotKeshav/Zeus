# Captures the same demo pages from one build on Windows, for the visual
# check that goes with the benchmark (macOS and Linux have shots.sh).
#
#   powershell -ExecutionPolicy Bypass -File scripts\bench\shots.ps1 [-Binary PATH] [-Label NAME] [-Out DIR]
#
# Writes bench-shots\<label>\{chat,light,pinned,narrow,calls}.png. The clock
# is frozen for screenshots, so two builds on the same machine should match:
#   python scripts\bench\imgdiff.py bench-shots\main bench-shots\pr66
param(
    [string]$Binary = "",
    [string]$Label = "",
    [string]$Out = ""
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "lib.ps1")

$Binary = Resolve-BenchBinary $Binary
if (-not (Test-BenchBuild $Binary "--demo-shot")) {
    Write-Error "$Binary is not a demo build (built without --features demo)"
    exit 3
}
if (-not $Label) { $Label = Get-BuildInfo $Binary "label" }
if (-not $Label) { Write-Error "Name this build, for example -Label main"; exit 2 }
if (-not $Out) { $Out = Join-Path (Get-Location) "bench-shots" }
$folder = Join-Path $Out $Label
New-Item -ItemType Directory -Force -Path $folder | Out-Null

$pages = @(
    @("chat", "chat", "1280x800"),
    @("light", "chat,light", "1280x800"),
    @("pinned", "pinned", "1280x800"),
    @("narrow", "chat", "600x800"),
    @("calls", "calls", "1280x800")
)
foreach ($page in $pages) {
    $file = Join-Path $folder "$($page[0]).png"
    Start-Process -FilePath $Binary -Wait `
        -ArgumentList @("--demo-shot", "`"$file`"", "--demo-page", $page[1], "--demo-size", $page[2])
    if (Test-Path $file) { Write-Host "  $($page[0])" } else { Write-Warning "$($page[0]): no picture" }
}
Write-Host "Wrote $folder"
