# Shared pieces of the benchmark's Windows sampler. Dot-sourced by
# sample.ps1, run.ps1 and shots.ps1; Windows PowerShell 5.1 or PowerShell 7.

$BenchPhases = @("settle", "scroll", "churn", "picker", "minimized", "settle-again")
# Seconds between readings, in every phase.
$BenchInterval = 2
# The scenario runs about 2 minutes; give up on a run well after that.
$BenchTimeout = 300

# The build to run: the given path, else the zeus.exe beside this folder as
# in a bench download.
function Resolve-BenchBinary([string]$Binary) {
    if (-not $Binary) {
        foreach ($candidate in @((Join-Path $PSScriptRoot "..\zeus.exe"), (Join-Path $PSScriptRoot "zeus.exe"))) {
            if (Test-Path $candidate) { $Binary = $candidate; break }
        }
    }
    if (-not $Binary -or -not (Test-Path $Binary)) {
        throw "Say which build to run: -Binary path\to\zeus.exe"
    }
    $Binary = (Resolve-Path $Binary).Path
    # A build downloaded in a browser is marked as coming from the internet.
    Unblock-File -Path $Binary -ErrorAction SilentlyContinue
    return $Binary
}

# Whether the build's --help lists `flag`, which only demo builds have.
function Test-BenchBuild([string]$Binary, [string]$Flag) {
    $help = [System.IO.Path]::GetTempFileName()
    try {
        # A release build has no console; redirecting gives its help somewhere to go.
        Start-Process -FilePath $Binary -ArgumentList "--help" -Wait -NoNewWindow `
            -RedirectStandardOutput $help -RedirectStandardError "$help.err" | Out-Null
        return (Get-Content -Raw -Path $help) -match [regex]::Escape($Flag)
    } catch {
        return $false
    } finally {
        Remove-Item -Force -ErrorAction SilentlyContinue $help, "$help.err"
    }
}

# A value from the BUILD_INFO.txt beside the build, or "".
function Get-BuildInfo([string]$Binary, [string]$Key) {
    $file = Join-Path (Split-Path $Binary) "BUILD_INFO.txt"
    if (-not (Test-Path $file)) { return "" }
    foreach ($line in Get-Content $file) {
        if ($line -match "^$([regex]::Escape($Key))=(.*)$") { return $Matches[1] }
    }
    return ""
}

# Bytes to MB (10^6 bytes), one decimal, as Task Manager counts.
function ConvertTo-MB([double]$Bytes) {
    return [math]::Round($Bytes / 1e6, 1)
}

# Private bytes (what the process has committed; Task Manager's "Commit
# size"), working set, and the peak working set, in bytes; and the CPU time
# used so far, in milliseconds.
function Get-BenchMemory([int]$ProcessId) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if (-not $process) { return $null }
    $process.Refresh()
    return [pscustomobject]@{
        Private = $process.PrivateMemorySize64
        WorkingSet = $process.WorkingSet64
        PeakWorkingSet = $process.PeakWorkingSet64
        CpuMs = $process.TotalProcessorTime.TotalMilliseconds
    }
}

# How busy the GPU's 3D engines were with this process since the last call,
# in percent (Task Manager's GPU column), from the "GPU Engine" performance
# counters of Windows 10 and later. $null on the first call and when the
# counters are not there.
$script:BenchGpuCounters = @()
function Read-BenchGpu([int]$ProcessId) {
    try {
        if (-not $script:BenchGpuCounters.Count) {
            $category = New-Object System.Diagnostics.PerformanceCounterCategory("GPU Engine")
            $names = @($category.GetInstanceNames() | Where-Object { $_ -like "pid_$($ProcessId)_*engtype_3D" })
            $script:BenchGpuCounters = @($names | ForEach-Object {
                $counter = New-Object System.Diagnostics.PerformanceCounter("GPU Engine", "Utilization Percentage", $_, $true)
                # A rate's first value is always 0; the next is since this one.
                [void]$counter.NextValue()
                $counter
            })
            return $null
        }
        $total = 0.0
        foreach ($counter in $script:BenchGpuCounters) { $total += $counter.NextValue() }
        return [math]::Round([math]::Min($total, 100.0), 1)
    } catch {
        $script:BenchGpuCounters = @()
        return $null
    }
}

# Names the renderer from the graphics libraries the process loaded.
function Get-BenchRenderer([string[]]$Modules) {
    $names = $Modules | ForEach-Object { $_.ToLowerInvariant() }
    if ($names -contains "vulkan-1.dll") { return "wgpu (Vulkan)" }
    if ($names -contains "d3d12.dll") { return "wgpu (DirectX 12)" }
    if ($names -contains "opengl32.dll") { return "OpenGL (glow)" }
    return "unknown"
}

# CPU time over wall time, in percent (100 is one core busy).
function Get-BenchPercent([double]$CpuMs, [double]$Ms) {
    if ($Ms -le 0) { return $null }
    return [math]::Round($CpuMs / $Ms * 100, 1)
}

# The phases' readings, in BenchPhases order, from samples (in the order
# taken) with Phase, Kind (tick or end), Ms since launch, Mb, OtherMb, CpuMs
# used since launch, and Gpu (percent or $null). A phase's CPU percent is
# over its whole length; its GPU percent is the mean of its readings.
function Get-BenchPhases($Samples) {
    $lastMs = 0; $lastCpu = 0; $endMs = 0; $endCpu = 0
    $points = @{}; $ends = @{}; $cpu = @{}
    foreach ($name in $BenchPhases) { $points[$name] = @() }
    foreach ($sample in $Samples) {
        $point = [ordered]@{
            t = [math]::Round($sample.Ms / 1000, 1); mb = $sample.Mb; other_mb = $sample.OtherMb
            cpu_pct = (Get-BenchPercent ($sample.CpuMs - $lastCpu) ($sample.Ms - $lastMs))
            gpu_pct = $sample.Gpu
        }
        $lastMs = $sample.Ms; $lastCpu = $sample.CpuMs
        if ($points.ContainsKey($sample.Phase)) { $points[$sample.Phase] += $point }
        if ($sample.Kind -eq "end") {
            $ends[$sample.Phase] = $point
            $cpu[$sample.Phase] = Get-BenchPercent ($sample.CpuMs - $endCpu) ($sample.Ms - $endMs)
            $endMs = $sample.Ms; $endCpu = $sample.CpuMs
        }
    }
    $phases = @()
    foreach ($name in $BenchPhases) {
        $mine = @($points[$name])
        $max = $null
        if ($mine.Count) { $max = ($mine | ForEach-Object { $_.mb } | Measure-Object -Maximum).Maximum }
        $gpu = @($mine | ForEach-Object { $_.gpu_pct } | Where-Object { $null -ne $_ })
        $phases += [ordered]@{
            name = $name
            end = $ends[$name]
            max_mb = $max
            cpu_pct = $cpu[$name]
            gpu_pct = $(if ($gpu.Count) { [math]::Round(($gpu | Measure-Object -Average).Average, 1) } else { $null })
            samples = $mine
        }
    }
    return $phases
}

# Keeps the display and the machine awake while the benchmark runs.
function Set-BenchAwake([bool]$Awake) {
    if (-not ("ZeusBench.Power" -as [type])) {
        Add-Type -Namespace ZeusBench -Name Power -MemberDefinition @"
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint SetThreadExecutionState(uint flags);
"@
    }
    # ES_CONTINUOUS, plus ES_SYSTEM_REQUIRED and ES_DISPLAY_REQUIRED while awake.
    $flags = [uint32]"0x80000000"
    if ($Awake) { $flags = [uint32]"0x80000003" }
    [ZeusBench.Power]::SetThreadExecutionState($flags) | Out-Null
}
