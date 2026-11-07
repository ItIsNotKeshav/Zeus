//! Opt-in process memory probe. Never on unless `ZAPFAST_MEMORY_LOG` is `1`.
//!
//! The variable is read once, on the first frame that asks. After that the
//! check is a single load, and a minute that passes with no frame logs
//! nothing: the probe never asks the window to repaint. The line is counts
//! and sizes only (account folder ids, never phone numbers, message text, or
//! keys). RSS includes every other field, so the numbers are a breakdown,
//! not parts to add together.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Switches the probe on. Any other value, including an empty one, leaves it off.
pub const VARIABLE: &str = "ZAPFAST_MEMORY_LOG";

/// How long a frame must wait after a sample before the next one.
const INTERVAL: Duration = Duration::from_secs(60);

/// One mebibyte, the unit the line reports RSS in.
const MIB: u64 = 1024 * 1024;

/// Loaded conversations of one account, for the sample line.
pub(crate) struct AccountCounts<'a> {
    pub id: &'a str,
    pub conversations: usize,
    pub messages: usize,
}

struct Sample {
    rss_mib: Option<u64>,
    textures: usize,
    font_atlas: [usize; 2],
    loader_bytes: usize,
    image_cache: usize,
    animation_frames: usize,
    accounts: Vec<AccountUsage>,
}

struct AccountUsage {
    id: String,
    conversations: usize,
    messages: usize,
}

/// Whether the probe was switched on. The variable is read once per process.
#[inline]
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(VARIABLE).is_some_and(|value| value == "1"))
}

/// Writes one line. The caller has already decided this frame should log.
///
/// `accounts` is walked only when a line is actually written, so pass the
/// iterator in only after [`background_due`] or [`interface_due`] says so.
pub(crate) fn sample<'a>(ctx: &egui::Context, accounts: impl Iterator<Item = AccountCounts<'a>>) {
    let accounts = accounts
        .map(|account| AccountUsage {
            id: account.id.to_owned(),
            conversations: account.conversations,
            messages: account.messages,
        })
        .collect();
    let sample = Sample {
        rss_mib: rss_bytes().map(|bytes| bytes / MIB),
        textures: ctx.tex_manager().read().num_allocated(),
        font_atlas: ctx.fonts(|fonts| fonts.font_image_size()),
        loader_bytes: loader_bytes(ctx),
        image_cache: crate::image_cache::len(ctx),
        animation_frames: crate::animation::resident_frames(ctx),
        accounts,
    };
    log::info!(target: "zeus::memory", "{}", format_sample(&sample));
}

struct Clock {
    last: Option<Instant>,
    /// A sample is due and is waiting for the interface frame.
    held: bool,
}

/// From `background_frame`. When an interface frame will follow, a due sample
/// is held for [`interface_due`] so the line is written after the image sweep.
/// A held sample the interface never takes is written on the next call.
pub(crate) fn background_due(interface_follows: bool) -> bool {
    with_clock(|clock| step_background(clock, Instant::now(), interface_follows))
}

/// From the interface frame, after the image sweep.
pub(crate) fn interface_due() -> bool {
    with_clock(|clock| step_interface(clock, Instant::now()))
}

fn with_clock(step: impl FnOnce(&mut Clock) -> bool) -> bool {
    if !enabled() {
        return false;
    }
    static CLOCK: Mutex<Clock> = Mutex::new(Clock {
        last: None,
        held: false,
    });
    let mut clock = CLOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    step(&mut clock)
}

fn step_background(clock: &mut Clock, now: Instant, interface_follows: bool) -> bool {
    if clock.held {
        clock.held = false;
        clock.last = Some(now);
        return true;
    }
    if !interval_elapsed(clock.last, now) {
        return false;
    }
    if interface_follows {
        clock.held = true;
        false
    } else {
        clock.last = Some(now);
        true
    }
}

fn step_interface(clock: &mut Clock, now: Instant) -> bool {
    if !clock.held && !interval_elapsed(clock.last, now) {
        return false;
    }
    clock.held = false;
    clock.last = Some(now);
    true
}

/// Whether `now` is the first sample or a full minute after `last`.
fn interval_elapsed(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= INTERVAL)
}

/// Bytes held by egui's image and texture loaders.
fn loader_bytes(ctx: &egui::Context) -> usize {
    let loaders = ctx.loaders();
    let image: usize = loaders
        .image
        .lock()
        .iter()
        .map(|loader| loader.byte_size())
        .sum();
    let texture: usize = loaders
        .texture
        .lock()
        .iter()
        .map(|loader| loader.byte_size())
        .sum();
    image.saturating_add(texture)
}

/// Resident set, in bytes. `None` when this platform cannot report it.
fn rss_bytes() -> Option<u64> {
    rss_bytes_impl()
}

#[cfg(target_os = "linux")]
fn rss_bytes_impl() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages = resident_pages(&statm)?;
    Some(pages.saturating_mul(page_size()))
}

#[cfg(target_os = "linux")]
fn page_size() -> u64 {
    // SAFETY: `_SC_PAGESIZE` returns the process page size, or -1.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    u64::try_from(page).unwrap_or(4096)
}

/// The resident field of `/proc/self/statm`, in pages. The columns are size,
/// resident, shared, text, lib, data, dt.
#[cfg(any(test, target_os = "linux"))]
fn resident_pages(statm: &str) -> Option<u64> {
    statm.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(target_os = "macos")]
fn rss_bytes_impl() -> Option<u64> {
    // SAFETY: `info` is a zeroed `mach_task_basic_info` and `count` is that
    // struct's word count, which is what `task_info` writes. The port is the
    // current task. `resident_size` is read unaligned because the struct is
    // packed to 4 bytes.
    unsafe {
        let mut info = std::mem::zeroed::<libc::mach_task_basic_info>();
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
        // libc deprecates this getter in favour of the `mach2` crate, which
        // this program does not depend on. The call only names the current task.
        #[allow(deprecated)]
        let task = libc::mach_task_self();
        let result = libc::task_info(
            task,
            libc::MACH_TASK_BASIC_INFO,
            (&raw mut info).cast(),
            &raw mut count,
        );
        if result != libc::KERN_SUCCESS {
            return None;
        }
        Some((&raw const info.resident_size).read_unaligned())
    }
}

#[cfg(windows)]
fn rss_bytes_impl() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: the counters are zeroed and `cb` is their size, which is what
    // `GetProcessMemoryInfo` writes. The handle is the current process
    // pseudo-handle and is not closed.
    unsafe {
        let mut counters = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        let size =
            u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap_or(u32::MAX);
        counters.cb = size;
        if GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, size) == 0 {
            return None;
        }
        u64::try_from(counters.WorkingSetSize).ok()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn rss_bytes_impl() -> Option<u64> {
    None
}

/// One log line. `rss_mib` is whole mebibytes, truncated; `accounts` is
/// `id:conversations/messages` for each account, or `-` when there are none.
fn format_sample(sample: &Sample) -> String {
    let rss = match sample.rss_mib {
        Some(mib) => mib.to_string(),
        None => "unavailable".to_owned(),
    };
    let accounts = if sample.accounts.is_empty() {
        "-".to_owned()
    } else {
        sample
            .accounts
            .iter()
            .map(|account| {
                format!(
                    "{}:{}/{}",
                    account.id, account.conversations, account.messages
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "rss_mib={rss} textures={} font_atlas={}x{} loader_bytes={} image_cache={} animation_frames={} accounts={accounts}",
        sample.textures,
        sample.font_atlas[0],
        sample.font_atlas[1],
        sample.loader_bytes,
        sample.image_cache,
        sample.animation_frames,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_probe_line() {
        let line = format_sample(&Sample {
            rss_mib: Some(214),
            textures: 96,
            font_atlas: [2048, 1024],
            loader_bytes: 12_345_678,
            image_cache: 40,
            animation_frames: 12,
            accounts: vec![
                AccountUsage {
                    id: "1".to_owned(),
                    conversations: 120,
                    messages: 45_000,
                },
                AccountUsage {
                    id: "2".to_owned(),
                    conversations: 3,
                    messages: 200,
                },
            ],
        });
        assert_eq!(
            line,
            "rss_mib=214 textures=96 font_atlas=2048x1024 loader_bytes=12345678 image_cache=40 animation_frames=12 accounts=1:120/45000,2:3/200"
        );
        let missing = format_sample(&Sample {
            rss_mib: None,
            textures: 0,
            font_atlas: [0, 0],
            loader_bytes: 0,
            image_cache: 0,
            animation_frames: 0,
            accounts: Vec::new(),
        });
        assert_eq!(
            missing,
            "rss_mib=unavailable textures=0 font_atlas=0x0 loader_bytes=0 image_cache=0 animation_frames=0 accounts=-"
        );
        assert!(!line.contains('\n') && !missing.contains('\n'));
    }

    #[test]
    fn samples_on_the_first_call_and_then_once_a_minute() {
        let start = Instant::now();
        assert!(interval_elapsed(None, start));
        assert!(!interval_elapsed(Some(start), start));
        assert!(!interval_elapsed(
            Some(start),
            start + Duration::from_secs(59)
        ));
        assert!(interval_elapsed(
            Some(start),
            start + Duration::from_secs(60)
        ));
    }

    #[test]
    fn the_interface_frame_writes_the_line_when_it_will_run() {
        let start = Instant::now();
        let mut clock = Clock {
            last: None,
            held: false,
        };
        assert!(!step_background(&mut clock, start, true));
        assert!(clock.held);
        assert!(step_interface(&mut clock, start));
        assert!(!clock.held);
        assert!(!step_interface(&mut clock, start + Duration::from_secs(59)));
        assert!(step_background(
            &mut clock,
            start + Duration::from_secs(60),
            false
        ));
    }

    #[test]
    fn a_held_sample_is_written_on_the_next_background_frame() {
        let start = Instant::now();
        let mut clock = Clock {
            last: None,
            held: false,
        };
        assert!(!step_background(&mut clock, start, true));
        assert!(step_background(
            &mut clock,
            start + Duration::from_millis(16),
            true
        ));
        assert!(!clock.held);
    }

    #[test]
    fn reads_the_resident_field_of_statm() {
        assert_eq!(resident_pages("1234 5678 90 1 0 2 0\n"), Some(5678));
        assert_eq!(resident_pages("1234"), None);
        assert_eq!(resident_pages("1234 no"), None);
    }

    #[test]
    fn this_process_reports_a_resident_set() {
        let bytes = rss_bytes().expect("resident set");
        assert!(bytes > 0, "{bytes}");
    }
}
