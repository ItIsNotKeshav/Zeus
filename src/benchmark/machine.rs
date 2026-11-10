//! The anonymous summary of the computer a benchmark ran on: the system and
//! its version, the processor, how many cores and how much memory, and whether
//! it ran on battery with energy saving on. Never the computer's or the user's
//! name, a serial number, or an address.

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

/// Whether the computer ran on its battery, and whether its energy saving is
/// on, when the run started. Battery saver throttles the processor and the
/// graphics, so a run on battery is not comparable with one plugged in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct Power {
    /// "battery", "ac", "no battery" or "unknown".
    pub source: String,
    /// The battery's charge in percent, where there is a battery that says.
    pub charge_pct: Option<u8>,
    /// Windows' Energy saver, macOS's Low Power Mode, or Linux's low-power
    /// platform profile; `None` where the system does not say.
    pub saver: Option<bool>,
}

impl Power {
    /// The words for the results line, such as "on battery (64%), Energy
    /// saver on". `None` when nothing is known.
    pub fn line(&self) -> Option<String> {
        let mut parts = Vec::new();
        match self.source.as_str() {
            "battery" => parts.push(match self.charge_pct {
                Some(charge) => format!("on battery ({charge}%)"),
                None => "on battery".to_owned(),
            }),
            "ac" => parts.push("plugged in".to_owned()),
            "no battery" => parts.push("no battery".to_owned()),
            _ => {}
        }
        match self.saver {
            Some(true) => parts.push("Energy saver on".to_owned()),
            Some(false) => parts.push("Energy saver off".to_owned()),
            None => {}
        }
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

/// The computer's power state now.
#[cfg(windows)]
pub fn power() -> Power {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    /// `BatteryFlag`'s bit for "no system battery".
    const NO_BATTERY: u8 = 128;
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: `status` is a SYSTEM_POWER_STATUS that the call fills in.
    if unsafe { GetSystemPowerStatus(&raw mut status) } == 0 {
        return Power {
            source: "unknown".into(),
            ..Power::default()
        };
    }
    let source = if status.BatteryFlag & NO_BATTERY != 0 {
        "no battery"
    } else {
        match status.ACLineStatus {
            0 => "battery",
            1 => "ac",
            _ => "unknown",
        }
    };
    Power {
        source: source.into(),
        charge_pct: (status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent),
        // Bit 0 of SystemStatusFlag is Energy saver (battery saver).
        saver: Some(status.SystemStatusFlag & 1 != 0),
    }
}

/// The computer's power state now.
#[cfg(target_os = "macos")]
pub fn power() -> Power {
    let pmset = |args: &[&str]| {
        std::process::Command::new("/usr/bin/pmset")
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    };
    let batteries = pmset(&["-g", "batt"]);
    let source = if batteries.contains("'AC Power'") {
        "ac"
    } else if batteries.contains("'Battery Power'") {
        "battery"
    } else {
        "unknown"
    };
    // The charge is the number before the first percent sign: "85%; discharging".
    let charge_pct = batteries.find('%').and_then(|at| {
        let digits: String = batteries[..at]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.chars().rev().collect::<String>().parse().ok()
    });
    // "lowpowermode 1" in the general settings when Low Power Mode is on.
    let saver = pmset(&["-g"]).lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("lowpowermode")).then(|| words.next() == Some("1"))
    });
    Power {
        source: source.into(),
        charge_pct,
        saver,
    }
}

/// The computer's power state now, from the kernel's power supply classes.
#[cfg(target_os = "linux")]
pub fn power() -> Power {
    let read = |dir: &std::path::Path, name: &str| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .map(|text| text.trim().to_owned())
    };
    let mut batteries = 0;
    let mut battery_discharging = false;
    let mut charge_pct: Option<u8> = None;
    let mut mains_online = false;
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let dir = entry.path();
            match read(&dir, "type").as_deref() {
                Some("Battery") => {
                    batteries += 1;
                    battery_discharging |= read(&dir, "status").as_deref() == Some("Discharging");
                    charge_pct = charge_pct.or_else(|| read(&dir, "capacity")?.parse().ok());
                }
                Some("Mains") if read(&dir, "online").as_deref() == Some("1") => {
                    mains_online = true;
                }
                _ => {}
            }
        }
    }
    let source = if batteries == 0 {
        "no battery"
    } else if mains_online {
        "ac"
    } else if battery_discharging {
        "battery"
    } else {
        "unknown"
    };
    // A platform profile of "low-power" is the closest Linux has to Energy saver.
    let saver = read(
        std::path::Path::new("/sys/firmware/acpi"),
        "platform_profile",
    )
    .map(|profile| profile == "low-power");
    Power {
        source: source.into(),
        charge_pct: charge_pct.filter(|charge| *charge <= 100),
        saver,
    }
}

/// The computer's power state now, where the system is not read.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn power() -> Power {
    Power {
        source: "unknown".into(),
        ..Power::default()
    }
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
