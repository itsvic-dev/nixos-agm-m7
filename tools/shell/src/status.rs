use zbus::blocking::{Connection, Proxy, fdo::ObjectManagerProxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

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
        let bars = connected_rssi(bus).map_or_else(|| bars_percent(strength), bars);
        return Ok(Some(Wifi { name: String::from_utf8_lossy(&ssid).into_owned(), bars }));
    }
    Ok(None)
}

const IWD: &str = "net.connman.iwd";

/// NetworkManager's iwd backend updates the AP strength only after a scan, so read the live RSSI from iwd.
pub fn connected_rssi(bus: &Connection) -> Option<i16> {
    let objects = ObjectManagerProxy::builder(bus).destination(IWD).ok()?.path("/").ok()?.build().ok()?;
    for (path, ifaces) in objects.get_managed_objects().ok()? {
        let Some(station) = ifaces.get("net.connman.iwd.Station") else { continue };
        let Some(network) = station
            .get("ConnectedNetwork")
            .and_then(|v| OwnedObjectPath::try_from(OwnedValue::try_clone(v).ok()?).ok())
        else {
            continue;
        };
        let proxy = Proxy::new(bus, IWD, path, "net.connman.iwd.Station").ok()?;
        let ordered: Vec<(OwnedObjectPath, i16)> = proxy.call("GetOrderedNetworks", &()).ok()?;
        return ordered.into_iter().find(|(p, _)| *p == network).map(|(_, r)| r);
    }
    None
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

pub fn bars(rssi: i16) -> u8 {
    match rssi {
        r if r >= -6000 => 4,
        r if r >= -6700 => 3,
        r if r >= -7400 => 2,
        r if r >= -8000 => 1,
        _ => 0,
    }
}
