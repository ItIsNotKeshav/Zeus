//! The anonymous summary of the computer a benchmark ran on: the system and
//! its version, the processor, how many cores and how much memory. Never the
//! computer's or the user's name, a serial number, or an address.

use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct Hardware {
    /// The maker's model name where the system offers it ("MacBookPro18,3").
    pub model: String,
    pub chip: String,
    pub cores: usize,
    /// Gigabytes (10^9 bytes), one decimal, as the scripts count them.
    pub ram_gb: f64,
    /// The graphics chip, as its OpenGL driver names it.
    pub gpu: String,
}

/// The system's name and version, and the hardware.
pub fn describe() -> (String, Hardware) {
    let cores = std::thread::available_parallelism().map_or(0, usize::from);
    let (os, model, chip, ram) = platform();
    (
        os,
        Hardware {
            model,
            chip,
            cores,
            ram_gb: (ram as f64 / 1e8).round() / 10.0,
            gpu: String::new(),
        },
    )
}

/// System, model, chip, and memory in bytes.
#[cfg(target_os = "linux")]
fn platform() -> (String, String, String, u64) {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let pretty = release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|value| value.trim_matches('"').to_owned())
        .unwrap_or_else(|| "Linux".to_owned());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let os = format!("{pretty} (Linux {})", kernel.trim());
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let chip = cpuinfo
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "model name").then(|| value.trim().to_owned())
        })
        .unwrap_or_default();
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let ram = super::probe::kib_field(&meminfo, "MemTotal:").unwrap_or(0) * 1024;
    let model = std::fs::read_to_string("/sys/class/dmi/id/product_name")
        .map(|name| name.trim().to_owned())
        .unwrap_or_default();
    (os, model, chip, ram)
}

#[cfg(target_os = "macos")]
fn platform() -> (String, String, String, u64) {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_default()
    };
    let version = run("/usr/bin/sw_vers", &["-productVersion"]);
    let os = format!("macOS {version} ({})", std::env::consts::ARCH);
    let chip = run("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"]);
    let model = run("/usr/sbin/sysctl", &["-n", "hw.model"]);
    let ram = run("/usr/sbin/sysctl", &["-n", "hw.memsize"])
        .parse()
        .unwrap_or(0);
    (os, model, chip, ram)
}

#[cfg(windows)]
fn platform() -> (String, String, String, u64) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let version = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let product = registry_string(version, "ProductName").unwrap_or_else(|| "Windows".into());
    let display = registry_string(version, "DisplayVersion").unwrap_or_default();
    let build = registry_string(version, "CurrentBuild").unwrap_or_default();
    // Windows 11 still calls itself Windows 10 here; its builds start at 22000.
    let product = match build.parse::<u32>() {
        Ok(number) if number >= 22000 => product.replace("Windows 10", "Windows 11"),
        _ => product,
    };
    let os = format!("{product} {display} (build {build})")
        .replace("  ", " ")
        .trim()
        .to_owned();
    let chip = registry_string(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
    .map(|name| name.trim().to_owned())
    .unwrap_or_default();
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `status` is a MEMORYSTATUSEX with its length set.
    let ram = if unsafe { GlobalMemoryStatusEx(&raw mut status) } != 0 {
        status.ullTotalPhys
    } else {
        0
    };
    (os, String::new(), chip, ram)
}

/// A string value under HKEY_LOCAL_MACHINE.
#[cfg(windows)]
fn registry_string(key: &str, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (wide(key), wide(name));
    let mut buffer = [0u16; 256];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: both names end in NUL; `size` is the buffer's size in bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let length = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform() -> (String, String, String, u64) {
    (
        std::env::consts::OS.to_owned(),
        String::new(),
        String::new(),
        0,
    )
}
