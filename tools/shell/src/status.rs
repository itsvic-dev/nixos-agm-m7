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

pub struct Mobile {
    /// `None` until the modem is registered on a network.
    pub bars: Option<u8>,
    /// Short network generation, for example "4G", or empty.
    pub tech: &'static str,
    pub operator: String,
    /// Mobile data is connected.
    pub data: bool,
}

#[derive(Default)]
pub struct Status {
    pub battery: Option<Battery>,
    pub wifi: Option<Wifi>,
    pub mobile: Option<Mobile>,
}

impl Status {
    pub fn read(bus: Option<&Connection>) -> Status {
        Status {
            battery: battery(),
            wifi: bus.and_then(|bus| wifi(bus).ok().flatten()),
            mobile: bus.and_then(|bus| mobile(bus).ok().flatten()),
        }
    }
}

const MM: &str = "org.freedesktop.ModemManager1";
const MM_MODEM: &str = "org.freedesktop.ModemManager1.Modem";
const MM_STATE_REGISTERED: i32 = 8;
const MM_STATE_CONNECTED: i32 = 11;

/// The network generation for a ModemManager access technology bitmask.
pub fn tech(access: u32) -> &'static str {
    match access {
        a if a & (1 << 15) != 0 => "5G",
        a if a & (1 << 14) != 0 => "4G",
        a if a & (1 << 9) != 0 => "H+",
        a if a & (0x7 << 6) != 0 => "H",
        a if a & (1 << 5) != 0 => "3G",
        a if a & (1 << 4) != 0 => "E",
        a if a & (1 << 3) != 0 => "G",
        a if a & 0x6 != 0 => "2G",
        _ => "",
    }
}

fn mobile(bus: &Connection) -> zbus::Result<Option<Mobile>> {
    let objects = ObjectManagerProxy::builder(bus).destination(MM)?.path("/org/freedesktop/ModemManager1")?.build()?.get_managed_objects()?;
    let Some(ifaces) = objects.values().find(|i| i.contains_key(MM_MODEM)) else { return Ok(None) };
    let modem = &ifaces[MM_MODEM];
    let get = |key: &str| modem.get(key).and_then(|v| v.try_clone().ok());
    let state = get("State").and_then(|v| i32::try_from(v).ok()).unwrap_or(0);
    let (quality, _recent): (u32, bool) = get("SignalQuality").and_then(|v| v.try_into().ok()).unwrap_or((0, false));
    let access = get("AccessTechnologies").and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    let operator = ifaces
        .get("org.freedesktop.ModemManager1.Modem.Modem3gpp")
        .and_then(|m| m.get("OperatorName"))
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_default();
    let registered = state >= MM_STATE_REGISTERED;
    Ok(Some(Mobile {
        bars: registered.then(|| bars_percent(quality.min(100) as u8)),
        tech: if registered { tech(access) } else { "" },
        operator,
        data: state == MM_STATE_CONNECTED,
    }))
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
