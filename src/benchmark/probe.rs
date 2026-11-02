//! Readings of the benchmark window's process, taken from outside it the way
//! `scripts/bench/sample.sh` and `sample.ps1` take them: memory as each
//! system counts what a process costs (PSS on Linux, the physical footprint on
//! macOS, private bytes on Windows), the CPU time it has used, and how busy
//! the GPU was where the system says.

use std::process::Child;
#[cfg(target_os = "linux")]
use std::time::Instant;

/// One reading of the process.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    /// Bytes, by the system's main count (see [`METRIC`]).
    pub memory: u64,
    /// Bytes, by the system's other count (see [`METRIC_OTHER`]).
    pub other: u64,
    /// The highest the system has seen, in bytes, where it keeps one.
    pub peak: u64,
    /// CPU time used since launch, in milliseconds.
    pub cpu_ms: f64,
    /// Percent; `None` on the first reading and where the system does not say.
    pub gpu: Option<f64>,
}

/// The main memory count's name, as the scripts' results files spell it.
pub const METRIC: &str = if cfg!(target_os = "macos") {
    "physical_footprint"
} else if cfg!(windows) {
    "private_bytes"
} else {
    "pss"
};
pub const METRIC_OTHER: &str = if cfg!(windows) { "working_set" } else { "rss" };
pub const PEAK_METRIC: &str = if cfg!(target_os = "macos") {
    "peak_physical_footprint"
} else if cfg!(windows) {
    "peak_working_set"
} else {
    "peak_rss"
};
/// macOS reports the whole GPU; the others the process's own 3D engine use.
pub const GPU_METRIC: &str = if cfg!(target_os = "macos") {
    "whole_gpu"
} else {
    "process_3d_engine"
};

/// Reads one process again and again.
#[derive(Default)]
pub struct Probe {
    /// GPU engine nanoseconds at the last reading, and when it was taken.
    #[cfg(target_os = "linux")]
    last_gpu: Option<(u64, Instant)>,
    #[cfg(windows)]
    gpu: Option<windows::GpuQuery>,
}

impl Probe {
    /// The process's readings now, or `None` once it is gone.
    pub fn read(&mut self, child: &Child) -> Option<Reading> {
        self.read_platform(child)
    }

    #[cfg(target_os = "linux")]
    fn read_platform(&mut self, child: &Child) -> Option<Reading> {
        let pid = child.id();
        let rollup = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fdinfo = drm_fdinfo(pid);
        let now = Instant::now();
        let gpu = fdinfo.and_then(|ns| {
            let previous = self.last_gpu.replace((ns, now));
            previous.and_then(|(before, then)| {
                let ms = (now - then).as_secs_f64() * 1000.0;
                (ms > 0.0).then(|| ((ns.saturating_sub(before)) as f64 / 1e4 / ms).min(100.0))
            })
        });
        Some(Reading {
            memory: kib_field(&rollup, "Pss:")? * 1024,
            other: kib_field(&rollup, "Rss:").unwrap_or(0) * 1024,
            peak: kib_field(&status, "VmHWM:").unwrap_or(0) * 1024,
            cpu_ms: cpu_ms_from_stat(&stat, clock_ticks())?,
            gpu: gpu.map(round1),
        })
    }

    #[cfg(target_os = "macos")]
    fn read_platform(&mut self, child: &Child) -> Option<Reading> {
        let usage = macos::rusage(child.id() as i32)?;
        Some(Reading {
            memory: usage.footprint,
            other: usage.resident,
            peak: usage.peak_footprint,
            cpu_ms: usage.cpu_ms,
            gpu: macos::gpu_utilisation(),
        })
    }

    #[cfg(windows)]
    fn read_platform(&mut self, child: &Child) -> Option<Reading> {
        let (memory, other, peak, cpu_ms) = windows::memory_and_cpu(child)?;
        let gpu = match &mut self.gpu {
            Some(query) => query.read(),
            None => {
                // The first collection only starts the rate.
                self.gpu = windows::GpuQuery::new(child.id());
                None
            }
        };
        Some(Reading {
            memory,
            other,
            peak,
            cpu_ms,
            gpu,
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    fn read_platform(&mut self, _child: &Child) -> Option<Reading> {
        None
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// A `Name:   1234 kB` line's number.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn kib_field(text: &str, name: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(name))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
}

/// User plus system CPU time from `/proc/<pid>/stat`, in milliseconds. The
/// command name in parentheses may hold spaces, so fields count from its end.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn cpu_ms_from_stat(stat: &str, ticks_per_second: f64) -> Option<f64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the name: state is field 3, utime 14 and stime 15.
    let user: f64 = fields.get(11)?.parse().ok()?;
    let system: f64 = fields.get(12)?.parse().ok()?;
    Some((user + system) * 1000.0 / ticks_per_second)
}

#[cfg(target_os = "linux")]
fn clock_ticks() -> f64 {
    // SAFETY: sysconf only reads a configuration value.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 { ticks as f64 } else { 100.0 }
}

/// Nanoseconds the GPU's 3D engine spent on the process, summed over its
/// DRM clients (amdgpu, Intel and other drivers that report drm-engine).
#[cfg(target_os = "linux")]
fn drm_fdinfo(pid: u32) -> Option<u64> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/fdinfo")).ok()?;
    let texts = entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok());
    parse_drm_fdinfo(texts)
}

/// The port of `bench_parse_drm_fdinfo` in scripts/bench/lib.sh: each client
/// counts once, however many descriptors share it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_drm_fdinfo(texts: impl IntoIterator<Item = String>) -> Option<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut total = 0u64;
    let mut found = false;
    for text in texts {
        let mut client = None;
        let mut busy = 0u64;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some("drm-client-id:"), Some(id)) => client = Some(id.to_owned()),
                (Some("drm-engine-render:" | "drm-engine-gfx:"), Some(ns)) => {
                    busy += ns.parse::<u64>().unwrap_or(0);
                }
                _ => {}
            }
        }
        if let Some(client) = client
            && seen.insert(client)
        {
            total += busy;
            found = true;
        }
    }
    found.then_some(total)
}

/// The port of `bench_parse_ioreg_gpu`: the busiest GPU's "Device
/// Utilization %" in `ioreg -r -d 1 -w 0 -c IOAccelerator`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_ioreg_gpu(text: &str) -> Option<f64> {
    const KEY: &str = "\"Device Utilization %\"=";
    let mut best: Option<f64> = None;
    let mut rest = text;
    while let Some(at) = rest.find(KEY) {
        rest = &rest[at + KEY.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(value) = digits.parse::<f64>() {
            best = Some(best.map_or(value, |best: f64| best.max(value)));
        }
    }
    best
}

#[cfg(target_os = "macos")]
mod macos {
    pub struct Usage {
        pub footprint: u64,
        pub resident: u64,
        pub peak_footprint: u64,
        pub cpu_ms: f64,
    }

    /// `proc_pid_rusage`, which reads a process of the same user without
    /// special rights, unlike `task_for_pid`.
    // libc marks its mach_timebase_info deprecated in favour of the mach2
    // crate; it is the same call, and not worth another dependency.
    #[allow(deprecated)]
    pub fn rusage(pid: i32) -> Option<Usage> {
        let mut info = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
        // SAFETY: `info` is a rusage_info_v4, which RUSAGE_INFO_V4 fills.
        let status =
            unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, info.as_mut_ptr().cast()) };
        if status != 0 {
            return None;
        }
        // SAFETY: filled by the successful call above.
        let info = unsafe { info.assume_init() };
        // CPU times are in Mach absolute time units, which are not
        // nanoseconds on Apple silicon.
        let mut base = libc::mach_timebase_info { numer: 1, denom: 1 };
        // SAFETY: writes the two fields of `base`.
        unsafe { libc::mach_timebase_info(&raw mut base) };
        let ns = (info.ri_user_time + info.ri_system_time) as f64 * f64::from(base.numer)
            / f64::from(base.denom.max(1));
        Some(Usage {
            footprint: info.ri_phys_footprint,
            resident: info.ri_resident_size,
            peak_footprint: info.ri_lifetime_max_phys_footprint,
            cpu_ms: ns / 1e6,
        })
    }

    pub fn gpu_utilisation() -> Option<f64> {
        let output = std::process::Command::new("/usr/sbin/ioreg")
            .args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        super::parse_ioreg_gpu(&String::from_utf8_lossy(&output.stdout))
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::{
            Performance::{
                PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
                PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
                PdhGetFormattedCounterArrayW, PdhOpenQueryW,
            },
            ProcessStatus::{
                GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
            },
            Threading::GetProcessTimes,
        },
    };

    /// Private bytes, working set, peak working set, and CPU milliseconds.
    pub fn memory_and_cpu(child: &std::process::Child) -> Option<(u64, u64, u64, f64)> {
        let handle = child.as_raw_handle();
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        // SAFETY: `handle` is the child's own, open while `child` lives, and
        // `counters` is as large as `cb` says.
        let ok = unsafe {
            GetProcessMemoryInfo(
                handle,
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                counters.cb,
            )
        };
        if ok == 0 {
            return None;
        }
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: as above; the four FILETIMEs are written.
        let ok = unsafe {
            GetProcessTimes(
                handle,
                &raw mut created,
                &raw mut exited,
                &raw mut kernel,
                &raw mut user,
            )
        };
        let ticks = |time: FILETIME| {
            (u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime)) as f64
        };
        // FILETIMEs count 100-nanosecond intervals.
        let cpu_ms = if ok == 0 {
            0.0
        } else {
            (ticks(kernel) + ticks(user)) / 10_000.0
        };
        Some((
            counters.PrivateUsage as u64,
            counters.WorkingSetSize as u64,
            counters.PeakWorkingSetSize as u64,
            cpu_ms,
        ))
    }

    /// The "GPU Engine" counters of the process's 3D engines (Task
    /// Manager's GPU column), by their English names so any display
    /// language finds them.
    pub struct GpuQuery {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
    }

    impl GpuQuery {
        pub fn new(pid: u32) -> Option<Self> {
            let path: Vec<u16> =
                format!("\\GPU Engine(pid_{pid}_*engtype_3D)\\Utilization Percentage\0")
                    .encode_utf16()
                    .collect();
            let mut query: PDH_HQUERY = std::ptr::null_mut();
            let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
            // SAFETY: plain PDH calls with out-pointers to locals; the path
            // ends in NUL. The query is closed in Drop.
            unsafe {
                if PdhOpenQueryW(std::ptr::null(), 0, &raw mut query) != 0 {
                    return None;
                }
                if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &raw mut counter) != 0 {
                    PdhCloseQuery(query);
                    return None;
                }
                PdhCollectQueryData(query);
            }
            Some(Self { query, counter })
        }

        /// The engines' summed use since the last call, in percent.
        pub fn read(&mut self) -> Option<f64> {
            // SAFETY: the query and counter are open; the buffer is sized by
            // PDH's own first answer and holds whole items.
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return None;
                }
                let mut size = 0u32;
                let mut count = 0u32;
                let status = PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &raw mut size,
                    &raw mut count,
                    std::ptr::null_mut(),
                );
                if status != PDH_MORE_DATA || size == 0 {
                    return None;
                }
                let item = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                let mut buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> =
                    Vec::with_capacity((size as usize).div_ceil(item));
                let status = PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &raw mut size,
                    &raw mut count,
                    buffer.as_mut_ptr(),
                );
                if status != 0 {
                    return None;
                }
                buffer.set_len(count as usize);
                let total: f64 = buffer
                    .iter()
                    .map(|item| item.FmtValue.Anonymous.doubleValue)
                    .sum();
                Some(super::round1(total.clamp(0.0, 100.0)))
            }
        }
    }

    impl Drop for GpuQuery {
        fn drop(&mut self) {
            // SAFETY: opened in `new` and closed once.
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_fields_parse_kib_and_cpu_time() {
        let rollup =
            "55d0-7ff [rollup]\nRss:              204800 kB\nPss:              150000 kB\n";
        assert_eq!(kib_field(rollup, "Pss:"), Some(150_000));
        assert_eq!(kib_field(rollup, "Rss:"), Some(204_800));
        assert_eq!(kib_field(rollup, "Swap:"), None);
        // A name with a space and a parenthesis, then utime 250 and stime 50.
        let stat = "4242 (zeus (bench)) S 1 4242 4242 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0";
        assert_eq!(cpu_ms_from_stat(stat, 100.0), Some(3000.0));
    }

    #[test]
    fn drm_clients_count_once_and_ioreg_takes_the_busiest_gpu() {
        let first = "pos:\t0\ndrm-client-id:\t7\ndrm-engine-render:\t1000 ns\n".to_owned();
        let shared = "pos:\t0\ndrm-client-id:\t7\ndrm-engine-render:\t1000 ns\n".to_owned();
        let other = "drm-client-id:\t9\ndrm-engine-gfx:\t500 ns\n".to_owned();
        assert_eq!(parse_drm_fdinfo([first, shared, other]), Some(1500));
        assert_eq!(parse_drm_fdinfo(["flags: 02\n".to_owned()]), None);
        let ioreg = r#"| "PerformanceStatistics" = {"Device Utilization %"=12,"Renderer Utilization %"=9}
| "PerformanceStatistics" = {"Device Utilization %"=31}"#;
        assert_eq!(parse_ioreg_gpu(ioreg), Some(31.0));
        assert_eq!(parse_ioreg_gpu("nothing"), None);
    }
}
