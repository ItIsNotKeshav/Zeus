# One run of the Zeus benchmark on Windows: launches a demo build on the
# two-minute benchmark scenario, measures its memory, CPU and GPU use from
# outside every 2 seconds and at the end of each phase, and writes one JSON
# file with those and the app's own frame times.
#
#   powershell -ExecutionPolicy Bypass -File scripts\bench\sample.ps1 -Binary PATH -Out FILE.json [-Label NAME] [-Run N]
#
# Usually started by run.ps1, which runs it several times. The app runs the
# synthetic demo only: offline sample chats in a fresh temporary folder,
# never a linked account, archive or contacts.
param(
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Label = "",
    [int]$Run = 1
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "lib.ps1")

$Binary = Resolve-BenchBinary $Binary
# Only a demo build can run the scenario; anything else is refused before it
# starts, so the benchmark can never open a linked account.
if (-not (Test-BenchBuild $Binary "--demo-tour-marks")) {
    Write-Host "$Binary is not a benchmark build (built without --features demo, or from a commit before the benchmark). Nothing was run."
    exit 3
}
if (-not $Label) { $Label = Get-BuildInfo $Binary "label" }
if (-not $Label) { Write-Host "Name this build with -Label (no BUILD_INFO.txt beside it)"; exit 2 }
$commit = Get-BuildInfo $Binary "commit"
if (-not $commit) {
    # A local build: the checkout's commit, when the build sits inside it.
    $commit = "unknown"
    $ErrorActionPreference = "Continue"
    try {
        $repo = (& git -C $PSScriptRoot rev-parse --show-toplevel 2>$null)
        if ($LASTEXITCODE -eq 0 -and $repo) {
            $repo = (Resolve-Path $repo).Path
            if ($Binary.StartsWith($repo, [System.StringComparison]::OrdinalIgnoreCase)) {
                $commit = (& git -C $repo rev-parse HEAD 2>$null)
                & git -C $repo diff --quiet HEAD -- src Cargo.toml Cargo.lock 2>$null
                if ($LASTEXITCODE -ne 0) { $commit = "$commit+changes" }
            }
        }
    } catch {
        $commit = "unknown"
    }
    $ErrorActionPreference = "Stop"
}
$changes = Get-BuildInfo $Binary "changes"

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("zeus-bench-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null
$marks = Join-Path $work "marks.jsonl"
$log = Join-Path $work "app.log"
New-Item -ItemType File -Path $marks | Out-Null

# The marks file is open for writing in the app; read it without locking it.
function Read-Marks {
    $stream = [System.IO.File]::Open($marks, "Open", "Read", "ReadWrite")
    try {
        $text = (New-Object System.IO.StreamReader($stream)).ReadToEnd()
    } finally {
        $stream.Dispose()
    }
    $lines = $text -split "`n"
    # The last piece is empty after a whole line, or a line being written.
    if ($lines.Count -le 1) { return @() }
    return $lines[0..($lines.Count - 2)]
}

$samples = New-Object System.Collections.ArrayList
$script:osPeak = 0
$started = Get-Date
$clock = [System.Diagnostics.Stopwatch]::StartNew()
function Add-Reading([string]$Phase, [string]$Kind) {
    $memory = Get-BenchMemory $process.Id
    if (-not $memory) { return }
    $gpu = Read-BenchGpu $process.Id
    $ms = $clock.ElapsedMilliseconds
    [void]$samples.Add([pscustomobject]@{
        Phase = $Phase; Kind = $Kind; Ms = $ms
        Mb = (ConvertTo-MB $memory.Private); OtherMb = (ConvertTo-MB $memory.WorkingSet)
        CpuMs = $memory.CpuMs; Gpu = $gpu
    })
    if ($memory.PeakWorkingSet -gt $script:osPeak) { $script:osPeak = $memory.PeakWorkingSet }
    Write-Host ("  {0,-13} {1,-4} {2,5:N1}s  {3,7} MB  GPU {4}" -f $Phase, $Kind, ($ms / 1000), (ConvertTo-MB $memory.Private), $(if ($null -ne $gpu) { "$gpu%" } else { "-" }))
}

Set-BenchAwake $true
$process = Start-Process -FilePath $Binary -PassThru `
    -RedirectStandardOutput $log -RedirectStandardError "$log.err" `
    -ArgumentList @("--demo-tour", "--demo-tour-script", "bench", "--demo-size", "1280x800",
        "--demo-tour-marks", "`"$marks`"")

$phase = $null; $seen = 0; $lastTick = Get-Date
$status = "running"; $reason = ""; $checks = @(); $frames = [ordered]@{}; $appStart = $null
$renderer = "unknown"
Write-Host "Run $Run of ${Label}: about 2 minutes. Leave the window alone until it closes."
try {
    while ($true) {
        if ($process.HasExited) {
            if ($status -eq "running") { $status = "failed"; $reason = "the app quit before the scenario ended" }
            break
        }
        if (((Get-Date) - $started).TotalSeconds -gt $BenchTimeout) {
            $status = "failed"; $reason = "timed out after $($BenchTimeout)s"
            break
        }
        $lines = @(Read-Marks)
        if ($lines.Count -gt $seen) {
            foreach ($line in $lines[$seen..($lines.Count - 1)]) {
                if (-not $line.Trim()) { continue }
                $mark = $line | ConvertFrom-Json
                switch ($mark.event) {
                    "start" { $appStart = $mark }
                    "phase" {
                        if ($phase) { Add-Reading $phase "end" }
                        $phase = $mark.phase
                        $lastTick = Get-Date
                    }
                    "check" {
                        $checks += $mark
                        if (-not $mark.ok) { Write-Host "  check failed: $($mark.name)" }
                    }
                    "frames" { $frames[$mark.phase] = $mark }
                    "end" {
                        if ($phase) { Add-Reading $phase "end" }
                        $phase = $null
                        $status = "complete"
                    }
                    "failed" {
                        $status = "failed"
                        $reason = "the scenario could not find something on screen; see the log"
                    }
                }
            }
            $seen = $lines.Count
            if ($status -ne "running") { break }
        }
        if ($phase -and ((Get-Date) - $lastTick).TotalSeconds -ge $BenchInterval) {
            $lastTick = Get-Date
            Add-Reading $phase "tick"
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $process.HasExited) {
        try {
            $modules = (Get-Process -Id $process.Id).Modules | ForEach-Object { $_.ModuleName }
            $renderer = Get-BenchRenderer $modules
        } catch { }
        # The app closes itself a few seconds after its last mark.
        if (-not $process.WaitForExit(30000)) { Stop-Process -Id $process.Id -Force }
    }
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    Set-BenchAwake $false
}

if ($status -ne "complete") {
    Write-Host "Run $Run failed: $reason. The app's last lines:"
    Get-Content -Tail 15 -Path $log, "$log.err" -ErrorAction SilentlyContinue | Write-Host
}

$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$computer = Get-CimInstance Win32_ComputerSystem
$gpu = Get-CimInstance Win32_VideoController | Select-Object -First 1
$settle = $samples | Where-Object { $_.Phase -eq "settle-again" -and $_.Kind -eq "end" } | Select-Object -Last 1
$peak = $null
if ($samples.Count) { $peak = ($samples | Measure-Object -Property Mb -Maximum).Maximum }
$result = [ordered]@{
    schema = 2
    label = $Label
    run = $Run
    status = $status
    reason = $reason
    commit = $commit
    changes = $changes
    started = $started.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    os = "$($os.Caption) $($os.Version)"
    hardware = [ordered]@{
        model = "$($computer.Manufacturer) $($computer.Model)"
        chip = "$($cpu.Name)".Trim()
        cores = [int]$computer.NumberOfLogicalProcessors
        ram_gb = [math]::Round($computer.TotalPhysicalMemory / 1e9, 1)
        gpu = "$($gpu.Name)"
    }
    renderer = $renderer
    app = $appStart
    metric = "private_bytes"
    metric_other = "working_set"
    gpu_metric = "process_3d_engine"
    phases = @(Get-BenchPhases $samples)
    peak_mb = $peak
    os_peak_mb = (ConvertTo-MB $script:osPeak)
    os_peak_metric = "peak_working_set"
    settled_mb = $(if ($settle) { $settle.Mb } else { $null })
    frames = $frames
    checks = @($checks)
}
$folder = Split-Path -Parent $Out
if ($folder) { New-Item -ItemType Directory -Force -Path $folder | Out-Null }
# UTF-8 without a byte order mark, which Windows PowerShell 5.1 would add.
[System.IO.File]::WriteAllText($Out, ($result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $work
Write-Host "Wrote $Out"
if ($status -ne "complete") { exit 1 }
