use std::process::Command;

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim_end_matches('\0').trim().to_string())
}

fn supply(dev: &str, attr: &str) -> Option<String> {
    read(&format!("/sys/class/power_supply/{dev}/{attr}"))
}

fn micro(dev: &str, attr: &str) -> Option<f64> {
    supply(dev, attr)?.parse::<f64>().ok().map(|v| v / 1_000_000.0)
}

pub fn battery() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut add = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            rows.push((k.to_string(), v));
        }
    };
    add("Level", supply("battery", "capacity").map(|v| format!("{v}%")));
    add("Status", supply("battery", "status"));
    add("Charger", supply("mt6357-charger", "online").map(|v| if v == "1" { "Connected (USB)".into() } else { "Not connected".into() }));
    add("Voltage", micro("battery", "voltage_now").map(|v| format!("{v:.2} V")));
    add("Current", micro("battery", "current_now").map(|v| format!("{:.0} mA", v * 1000.0)));
    add("Temperature", supply("battery", "temp").and_then(|v| v.parse::<f64>().ok()).map(|t| format!("{:.1} °C", t / 10.0)));
    add("Capacity", micro("battery", "charge_full_design").map(|v| format!("{:.0} mAh", v * 1000.0)));
    add("Technology", supply("battery", "technology"));
    add("Charger health", supply("hl7005", "health"));
    rows
}

pub fn storage() -> Vec<(String, u64, u64)> {
    [("System", "/"), ("Data", "/data"), ("Nix store", "/nix/store")]
        .iter()
        .filter_map(|(name, path)| {
            let c = std::ffi::CString::new(*path).ok()?;
            let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
                return None;
            }
            let block = st.f_frsize as u64;
            let total = st.f_blocks as u64 * block;
            let free = st.f_bavail as u64 * block;
            Some((name.to_string(), total - free, total))
        })
        .fold(Vec::new(), |mut acc: Vec<(String, u64, u64)>, item| {
            // Mount points on the same filesystem report the same numbers.
            if !acc.iter().any(|a| a.1 == item.1 && a.2 == item.2) {
                acc.push(item);
            }
            acc
        })
}

pub fn size(bytes: u64) -> String {
    let gib = bytes as f64 / (1u64 << 30) as f64;
    if gib >= 1.0 { format!("{gib:.1} GB") } else { format!("{:.0} MB", bytes as f64 / (1u64 << 20) as f64) }
}

fn os_release(key: &str) -> Option<String> {
    read("/etc/os-release")?
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")).map(|v| v.trim_matches('"').to_string()))
}

fn meminfo(key: &str) -> Option<u64> {
    read("/proc/meminfo")?
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}:"))?.trim().trim_end_matches(" kB").parse().ok())
}

fn uptime() -> Option<String> {
    let secs = read("/proc/uptime")?.split('.').next()?.parse::<u64>().ok()?;
    let (d, h, m) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60);
    Some(if d > 0 { format!("{d} d {h} h {m} min") } else { format!("{h} h {m} min") })
}

fn ip(dev: &str) -> Option<String> {
    let out = Command::new("ip").args(["-o", "-4", "addr", "show", dev]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    Some(text.split_whitespace().skip_while(|w| *w != "inet").nth(1)?.split('/').next()?.to_string())
}

/// The board name from the first DT `compatible` entry, because LK overwrites `model`.
pub fn model() -> Option<String> {
    let compatible = read("/proc/device-tree/compatible")?;
    let (vendor, board) = compatible.split('\0').next()?.split_once(',')?;
    Some(format!("{} {}", vendor.to_uppercase(), board.to_uppercase()))
}

pub fn about() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut add = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            rows.push((k.to_string(), v));
        }
    };
    add("Model", model());
    add("Software", os_release("PRETTY_NAME"));
    add(
        "Generation",
        std::fs::read_link("/nix/var/nix/profiles/system").ok().and_then(|p| {
            p.to_string_lossy().strip_prefix("system-")?.strip_suffix("-link").map(|g| format!("#{g}"))
        }),
    );
    add("Kernel", read("/proc/sys/kernel/osrelease"));
    add("Chip", Some("MediaTek MT6739".into()));
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let max = read("/sys/devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq")
        .and_then(|f| f.parse::<f64>().ok())
        .map(|khz| format!(", up to {:.2} GHz", khz / 1_000_000.0))
        .unwrap_or_default();
    add("CPU", Some(format!("{cores}x Cortex-A53{max}")));
    let gpu = if std::path::Path::new("/sys/module/powervr").exists() { "PowerVR GE8100" } else { "PowerVR GE8100 (off)" };
    add("GPU", Some(gpu.into()));
    add("Memory", meminfo("MemTotal").map(|kb| format!("{} MB", kb / 1024)));
    add("Display", Some("240 x 320".into()));
    add("Wi-Fi address", read("/sys/class/net/wlan0/address"));
    add("IP address", ip("wlan0"));
    add("Host name", read("/proc/sys/kernel/hostname"));
    add("Uptime", uptime());
    rows
}
