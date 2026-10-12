use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";

const DEVICE_TYPE_WIFI: u32 = 2;
const KEY_MGMT_802_1X: u32 = 0x200;
const AP_FLAGS_PRIVACY: u32 = 0x1;

pub enum Cmd {
    Refresh,
    Power(bool),
    Scan,
    Connect { network: OwnedObjectPath, passphrase: Option<String> },
    Disconnect,
    Forget(OwnedObjectPath),
}

#[derive(Clone)]
pub struct Network {
    pub path: OwnedObjectPath,
    pub name: String,
    pub kind: String,
    pub bars: u8,
    pub known: Option<OwnedObjectPath>,
    pub connected: bool,
}

#[derive(Clone, Default)]
pub struct State {
    pub available: bool,
    pub powered: bool,
    pub scanning: bool,
    pub state: String,
    pub networks: Vec<Network>,
    device: Option<OwnedObjectPath>,
}

pub enum Event {
    State(State),
    Message(String),
}

pub struct Wifi {
    tx: mpsc::Sender<Cmd>,
}

impl Wifi {
    pub fn start(events: Sender<Event>) -> Wifi {
        let (tx, rx) = mpsc::channel();
        let changed = tx.clone();
        agm_shell::netwatch::watch(move || changed.send(Cmd::Refresh).is_ok());
        std::thread::spawn(move || run(rx, events));
        Wifi { tx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

fn run(rx: mpsc::Receiver<Cmd>, events: Sender<Event>) {
    let bus = match Connection::system() {
        Ok(bus) => bus,
        Err(e) => {
            let _ = events.send(Event::Message(format!("No system bus: {e}")));
            return;
        }
    };

    let mut state = read_state(&bus).unwrap_or_default();
    let mut scan_until = Instant::now();
    if handle(&bus, &state, Cmd::Scan, &events).is_ok() {
        scan_until = Instant::now() + Duration::from_secs(5);
    }
    loop {
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(cmd) => {
                if matches!(cmd, Cmd::Scan) {
                    scan_until = Instant::now() + Duration::from_secs(5);
                }
                if let Err(e) = handle(&bus, &state, cmd, &events) {
                    let _ = events.send(Event::Message(describe(&e)));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        state = read_state(&bus).unwrap_or_default();
        state.scanning = Instant::now() < scan_until;
        if events.send(Event::State(state.clone())).is_err() {
            return;
        }
    }
}

fn nm(bus: &Connection) -> zbus::Result<Proxy<'_>> {
    Proxy::new(bus, NM, NM_PATH, NM)
}

fn handle(bus: &Connection, state: &State, cmd: Cmd, events: &Sender<Event>) -> zbus::Result<()> {
    if let Cmd::Refresh = cmd {
        return Ok(());
    }
    if let Cmd::Power(on) = cmd {
        return nm(bus)?.set_property("WirelessEnabled", on).map_err(Into::into);
    }
    let device = state.device.clone().ok_or(zbus::Error::Failure("no Wi-Fi device".into()))?;
    match cmd {
        Cmd::Refresh | Cmd::Power(_) => unreachable!(),
        Cmd::Scan => {
            let opts: HashMap<&str, Value> = HashMap::new();
            Proxy::new(bus, NM, device, WIRELESS)?.call("RequestScan", &(opts,))
        }
        Cmd::Connect { network, passphrase } => connect(bus, &device, network, passphrase, events),
        Cmd::Disconnect => Proxy::new(bus, NM, device, DEVICE)?.call("Disconnect", &()),
        Cmd::Forget(known) => Proxy::new(bus, NM, known, CONNECTION)?.call("Delete", &()),
    }
}

fn connect(
    bus: &Connection,
    device: &OwnedObjectPath,
    ap: OwnedObjectPath,
    passphrase: Option<String>,
    events: &Sender<Event>,
) -> zbus::Result<()> {
    let ssid: Vec<u8> = Proxy::new(bus, NM, ap.clone(), AP)?.get_property("Ssid")?;
    let known = known_connections(bus)?.remove(&ssid);
    let (active, added) = match (known, passphrase) {
        (Some(conn), None) => {
            let active: OwnedObjectPath = nm(bus)?.call("ActivateConnection", &(conn, device, ap))?;
            (active, None)
        }
        (_, passphrase) => {
            let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();
            if let Some(psk) = passphrase {
                settings.insert(
                    "802-11-wireless-security",
                    HashMap::from([("key-mgmt", Value::from("wpa-psk")), ("psk", Value::from(psk))]),
                );
            }
            let (conn, active): (OwnedObjectPath, OwnedObjectPath) =
                nm(bus)?.call("AddAndActivateConnection", &(settings, device, ap))?;
            (active, Some(conn))
        }
    };

    // A wrong password does not fail the call; the activation fails later.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        let state: Option<u32> = Proxy::new(bus, NM, active.clone(), ACTIVE)?.get_property("State").ok();
        match state {
            Some(2) => return Ok(()),
            Some(1) => continue,
            _ => break,
        }
    }
    if let Some(conn) = added {
        let _ = Proxy::new(bus, NM, conn, CONNECTION)?.call::<_, _, ()>("Delete", &());
    }
    let _ = events.send(Event::Message("Could not connect. Wrong password?".into()));
    Ok(())
}

fn describe(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(_, Some(desc), _) => desc.to_string(),
        zbus::Error::MethodError(name, None, _) => name.as_str().rsplit('.').next().unwrap_or("Error").to_string(),
        e => e.to_string(),
    }
}

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

/// Saved Wi-Fi connections by SSID.
fn known_connections(bus: &Connection) -> zbus::Result<HashMap<Vec<u8>, OwnedObjectPath>> {
    let paths: Vec<OwnedObjectPath> =
        Proxy::new(bus, NM, "/org/freedesktop/NetworkManager/Settings", SETTINGS)?.call("ListConnections", &())?;
    let mut known = HashMap::new();
    for path in paths {
        let settings: Settings = Proxy::new(bus, NM, path.clone(), CONNECTION)?.call("GetSettings", &())?;
        let ssid = settings
            .get("802-11-wireless")
            .and_then(|w| w.get("ssid"))
            .and_then(|v| Vec::<u8>::try_from(v.try_clone().ok()?).ok());
        if let Some(ssid) = ssid {
            known.insert(ssid, path);
        }
    }
    Ok(known)
}

fn device_state(code: u32) -> &'static str {
    match code {
        100 => "connected",
        40..=90 => "connecting",
        110 | 120 => "disconnecting",
        30 => "disconnected",
        _ => "unavailable",
    }
}

fn read_state(bus: &Connection) -> zbus::Result<State> {
    let nm = nm(bus)?;
    let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &())?;
    let mut device = None;
    for path in devices {
        let kind: u32 = Proxy::new(bus, NM, path.clone(), DEVICE)?.get_property("DeviceType")?;
        if kind == DEVICE_TYPE_WIFI {
            device = Some(path);
            break;
        }
    }
    let Some(device) = device else { return Ok(State::default()) };

    let dev = Proxy::new(bus, NM, device.clone(), DEVICE)?;
    let wireless = Proxy::new(bus, NM, device.clone(), WIRELESS)?;
    let mut state = State {
        available: true,
        powered: nm.get_property("WirelessEnabled").unwrap_or(false),
        scanning: false,
        state: device_state(dev.get_property("State").unwrap_or(0)).into(),
        networks: Vec::new(),
        device: Some(device),
    };
    if !state.powered {
        return Ok(state);
    }

    let active: OwnedObjectPath = wireless.get_property("ActiveAccessPoint")?;
    let active_ssid: Option<Vec<u8>> = (active.as_str() != "/")
        .then(|| Proxy::new(bus, NM, active.clone(), AP).ok()?.get_property("Ssid").ok())
        .flatten();
    let known = known_connections(bus).unwrap_or_default();
    let aps: Vec<OwnedObjectPath> = wireless.call("GetAllAccessPoints", &())?;

    let mut best: HashMap<Vec<u8>, (u8, Network)> = HashMap::new();
    for path in aps {
        let ap = Proxy::new(bus, NM, path.clone(), AP)?;
        let ssid: Vec<u8> = ap.get_property("Ssid")?;
        if ssid.is_empty() {
            continue;
        }
        let strength: u8 = ap.get_property("Strength").unwrap_or(0);
        let flags: u32 = ap.get_property("Flags").unwrap_or(0);
        let wpa: u32 = ap.get_property("WpaFlags").unwrap_or(0);
        let rsn: u32 = ap.get_property("RsnFlags").unwrap_or(0);
        let kind = if (wpa | rsn) & KEY_MGMT_802_1X != 0 {
            "8021x"
        } else if flags & AP_FLAGS_PRIVACY != 0 || wpa | rsn != 0 {
            "psk"
        } else {
            "open"
        };
        if best.get(&ssid).is_some_and(|(s, _)| *s >= strength) {
            continue;
        }
        let network = Network {
            name: String::from_utf8_lossy(&ssid).into_owned(),
            kind: kind.into(),
            bars: agm_shell::status::bars_percent(strength),
            known: known.get(&ssid).cloned(),
            connected: active_ssid.as_ref() == Some(&ssid),
            path,
        };
        let network = if network.connected {
            let bars = agm_shell::status::connected_rssi(bus).map_or(network.bars, agm_shell::status::bars);
            Network { bars, ..network }
        } else {
            network
        };
        best.insert(ssid, (strength, network));
    }
    let mut networks: Vec<(u8, Network)> = best.into_values().collect();
    networks.sort_by(|a, b| b.1.connected.cmp(&a.1.connected).then(b.0.cmp(&a.0)));
    state.networks = networks.into_iter().map(|(_, n)| n).collect();
    Ok(state)
}
