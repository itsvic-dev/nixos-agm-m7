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

fn battery() -> Option<Battery> {
    let read = |f: &str| std::fs::read_to_string(format!("/sys/class/power_supply/battery/{f}")).ok();
    let percent = read("capacity")?.trim().parse().ok()?;
    let charging = matches!(read("status").as_deref().map(str::trim), Some("Charging" | "Full"));
    Some(Battery { percent, charging })
}

const IWD: &str = "net.connman.iwd";

fn wifi(bus: &Connection) -> zbus::Result<Option<Wifi>> {
    let objects = ObjectManagerProxy::builder(bus).destination(IWD)?.path("/")?.build()?.get_managed_objects()?;

    let prop = |path: &OwnedObjectPath, iface: &str, name: &str| -> Option<OwnedValue> {
        objects.get(path)?.get(iface)?.get(name).and_then(|v| v.try_clone().ok())
    };

    for (path, ifaces) in &objects {
        let Some(station) = ifaces.get("net.connman.iwd.Station") else { continue };
        let connected = station.get("State").and_then(|v| String::try_from(v.try_clone().ok()?).ok());
        if connected.as_deref() != Some("connected") {
            continue;
        }
        let Some(network) = station
            .get("ConnectedNetwork")
            .and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok())
        else {
            continue;
        };
        let name = prop(&network, "net.connman.iwd.Network", "Name")
            .and_then(|v| String::try_from(v).ok())
            .unwrap_or_default();

        let proxy = Proxy::new(bus, IWD, path.as_ref(), "net.connman.iwd.Station")?;
        let ordered: Vec<(OwnedObjectPath, i16)> = proxy.call("GetOrderedNetworks", &())?;
        let rssi = ordered.iter().find(|(p, _)| *p == network).map(|(_, r)| *r);
        return Ok(Some(Wifi { name, bars: rssi.map_or(0, bars) }));
    }
    Ok(None)
}

fn bars(rssi: i16) -> u8 {
    match rssi {
        r if r >= -6000 => 4,
        r if r >= -6700 => 3,
        r if r >= -7400 => 2,
        r if r >= -8000 => 1,
        _ => 0,
    }
}
