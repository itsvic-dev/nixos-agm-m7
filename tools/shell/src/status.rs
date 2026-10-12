use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

pub struct Wifi {
    pub name: String,
    pub bars: u8,
}

#[derive(Default)]
pub struct Status {
    pub battery: Option<Battery>,
    pub wifi: Option<Wifi>,
}

impl Status {
    pub fn read(bus: Option<&Connection>) -> Status {
        Status {
            battery: battery(),
            wifi: bus.and_then(|bus| wifi(bus).ok().flatten()),
        }
    }
}

pub fn battery() -> Option<Battery> {
    let read = |f: &str| std::fs::read_to_string(format!("/sys/class/power_supply/battery/{f}")).ok();
    let percent = read("capacity")?.trim().parse().ok()?;
    let charging = matches!(read("status").as_deref().map(str::trim), Some("Charging" | "Full"));
    Some(Battery { percent, charging })
}

const NM: &str = "org.freedesktop.NetworkManager";

fn wifi(bus: &Connection) -> zbus::Result<Option<Wifi>> {
    let nm = Proxy::new(bus, NM, "/org/freedesktop/NetworkManager", NM)?;
    let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &())?;
    for path in devices {
        let device = Proxy::new(bus, NM, path.clone(), "org.freedesktop.NetworkManager.Device")?;
        let kind: u32 = device.get_property("DeviceType")?;
        let state: u32 = device.get_property("State")?;
        if kind != 2 || state != 100 {
            continue;
        }
        let wireless = Proxy::new(bus, NM, path, "org.freedesktop.NetworkManager.Device.Wireless")?;
        let ap: OwnedObjectPath = wireless.get_property("ActiveAccessPoint")?;
        if ap.as_str() == "/" {
            continue;
        }
        let ap = Proxy::new(bus, NM, ap, "org.freedesktop.NetworkManager.AccessPoint")?;
        let ssid: Vec<u8> = ap.get_property("Ssid")?;
        let strength: u8 = ap.get_property("Strength").unwrap_or(0);
        return Ok(Some(Wifi { name: String::from_utf8_lossy(&ssid).into_owned(), bars: bars_percent(strength) }));
    }
    Ok(None)
}

pub fn bars_percent(strength: u8) -> u8 {
    match strength {
        s if s >= 75 => 4,
        s if s >= 55 => 3,
        s if s >= 35 => 2,
        s if s >= 15 => 1,
        _ => 0,
    }
}
