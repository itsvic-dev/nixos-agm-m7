use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use calloop::channel::Sender;
use zbus::blocking::{Connection, Proxy, fdo::ObjectManagerProxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const NM_CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const NM_ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const NM_DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const NM_DEVICE_TYPE_MODEM: u32 = 8;
const MM: &str = "org.freedesktop.ModemManager1";
const MM_MODEM: &str = "org.freedesktop.ModemManager1.Modem";
const MM_3GPP: &str = "org.freedesktop.ModemManager1.Modem.Modem3gpp";
const DEFAULT_APN: &str = "internet";

const MODE_2G: u32 = 1 << 1;
const MODE_3G: u32 = 1 << 2;
const MODE_4G: u32 = 1 << 3;

/// Network type choices: label, allowed modes, preferred mode.
pub const MODES: [(&str, u32, u32); 3] = [
    ("4G, 3G, 2G", MODE_4G | MODE_3G | MODE_2G, MODE_4G),
    ("3G, 2G", MODE_3G | MODE_2G, MODE_3G),
    ("2G only", MODE_2G, 0),
];

pub enum Cmd {
    Refresh,
    Data(bool),
    Roaming(bool),
    Mode(usize),
    Apn(String),
}

#[derive(Clone, Default)]
pub struct State {
    pub available: bool,
    pub status: String,
    pub operator: String,
    pub tech: String,
    pub signal: u32,
    pub sim: String,
    pub imei: String,
    pub data_enabled: bool,
    pub data_active: bool,
    pub roaming: bool,
    pub apn: String,
    pub mode: Option<usize>,
    pub modes: Vec<usize>,
}

pub enum Event {
    State(State),
    Message(String),
}

pub struct Mobile {
    tx: mpsc::Sender<Cmd>,
}

impl Mobile {
    pub fn start(events: Sender<Event>) -> Mobile {
        let (tx, rx) = mpsc::channel();
        let changed = tx.clone();
        agm_shell::netwatch::watch(move || changed.send(Cmd::Refresh).is_ok());
        std::thread::spawn(move || run(rx, events));
        Mobile { tx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

fn run(rx: mpsc::Receiver<Cmd>, events: Sender<Event>) {
    let Ok(bus) = Connection::system() else { return };
    loop {
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(cmd) => {
                if let Err(e) = handle(&bus, cmd) {
                    let _ = events.send(Event::Message(describe(&e)));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if events.send(Event::State(read_state(&bus).unwrap_or_default())).is_err() {
            return;
        }
    }
}

fn describe(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(_, Some(desc), _) => desc.to_string(),
        e => e.to_string(),
    }
}

fn value(v: impl Into<Value<'static>>) -> OwnedValue {
    v.into().try_to_owned().expect("plain value")
}

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

fn nm(bus: &Connection) -> zbus::Result<Proxy<'_>> {
    Proxy::new(bus, NM, NM_PATH, NM)
}

/// The first saved mobile broadband connection and its settings.
fn gsm_connection(bus: &Connection) -> zbus::Result<Option<(OwnedObjectPath, Settings)>> {
    let paths: Vec<OwnedObjectPath> =
        Proxy::new(bus, NM, "/org/freedesktop/NetworkManager/Settings", NM_SETTINGS)?.call("ListConnections", &())?;
    for path in paths {
        let settings: Settings = Proxy::new(bus, NM, path.clone(), NM_CONNECTION)?.call("GetSettings", &())?;
        let kind = settings
            .get("connection")
            .and_then(|c| c.get("type"))
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok());
        if kind.as_deref() == Some("gsm") {
            return Ok(Some((path, settings)));
        }
    }
    Ok(None)
}

fn gsm_connection_or_new(bus: &Connection) -> zbus::Result<(OwnedObjectPath, Settings)> {
    if let Some(found) = gsm_connection(bus)? {
        return Ok(found);
    }
    let settings: HashMap<&str, HashMap<&str, OwnedValue>> = HashMap::from([
        (
            "connection",
            HashMap::from([("id", value("mobile")), ("type", value("gsm")), ("autoconnect", value(false))]),
        ),
        ("gsm", HashMap::from([("apn", value(DEFAULT_APN))])),
        ("ipv6", HashMap::from([("method", value("disabled"))])),
    ]);
    let _: OwnedObjectPath =
        Proxy::new(bus, NM, "/org/freedesktop/NetworkManager/Settings", NM_SETTINGS)?.call("AddConnection", &(settings,))?;
    gsm_connection(bus)?.ok_or(zbus::Error::Failure("could not create the mobile connection".into()))
}

/// Changes a saved connection. Secrets are not in GetSettings, so they are merged back first.
fn update(bus: &Connection, path: &OwnedObjectPath, mut settings: Settings, change: impl FnOnce(&mut Settings)) -> zbus::Result<()> {
    let conn = Proxy::new(bus, NM, path.clone(), NM_CONNECTION)?;
    if let Ok(secrets) = conn.call::<_, _, Settings>("GetSecrets", &("gsm",)) {
        for (group, values) in secrets {
            settings.entry(group).or_default().extend(values);
        }
    }
    for group in ["ipv4", "ipv6"] {
        if let Some(ip) = settings.get_mut(group) {
            ip.remove("addresses");
            ip.remove("routes");
        }
    }
    change(&mut settings);
    conn.call("Update", &(settings,))
}

fn set(settings: &mut Settings, group: &str, key: &str, v: OwnedValue) {
    settings.entry(group.into()).or_default().insert(key.into(), v);
}

/// The active connection object of a saved connection.
fn active_of(bus: &Connection, path: &OwnedObjectPath) -> zbus::Result<Option<OwnedObjectPath>> {
    let actives: Vec<OwnedObjectPath> = nm(bus)?.get_property("ActiveConnections")?;
    for active in actives {
        let conn: OwnedObjectPath = Proxy::new(bus, NM, active.clone(), NM_ACTIVE)?.get_property("Connection")?;
        if conn == *path {
            return Ok(Some(active));
        }
    }
    Ok(None)
}

fn modem_device(bus: &Connection) -> zbus::Result<OwnedObjectPath> {
    let devices: Vec<OwnedObjectPath> = nm(bus)?.call("GetDevices", &())?;
    for device in devices {
        let kind: u32 = Proxy::new(bus, NM, device.clone(), NM_DEVICE)?.get_property("DeviceType")?;
        if kind == NM_DEVICE_TYPE_MODEM {
            return Ok(device);
        }
    }
    Err(zbus::Error::Failure("The modem is not ready".into()))
}

fn activate(bus: &Connection, path: &OwnedObjectPath) -> zbus::Result<()> {
    let device = modem_device(bus)?;
    let root = OwnedObjectPath::try_from("/").unwrap();
    nm(bus)?.call::<_, _, OwnedObjectPath>("ActivateConnection", &(path, device, root)).map(|_| ())
}

fn deactivate(bus: &Connection, path: &OwnedObjectPath) -> zbus::Result<()> {
    match active_of(bus, path)? {
        Some(active) => nm(bus)?.call("DeactivateConnection", &(active,)),
        None => Ok(()),
    }
}

type Interfaces = HashMap<zbus::names::OwnedInterfaceName, HashMap<String, OwnedValue>>;

fn modem(bus: &Connection) -> zbus::Result<Option<(OwnedObjectPath, Interfaces)>> {
    let objects = ObjectManagerProxy::builder(bus).destination(MM)?.path("/org/freedesktop/ModemManager1")?.build()?.get_managed_objects()?;
    Ok(objects.into_iter().find(|(_, i)| i.contains_key(MM_MODEM)))
}

fn handle(bus: &Connection, cmd: Cmd) -> zbus::Result<()> {
    match cmd {
        Cmd::Refresh => Ok(()),
        Cmd::Data(on) => {
            let (path, settings) = gsm_connection_or_new(bus)?;
            update(bus, &path, settings, |s| set(s, "connection", "autoconnect", value(on)))?;
            if on { activate(bus, &path) } else { deactivate(bus, &path) }
        }
        Cmd::Roaming(allowed) => {
            let (path, settings) = gsm_connection_or_new(bus)?;
            update(bus, &path, settings, |s| set(s, "gsm", "home-only", value(!allowed)))
        }
        Cmd::Apn(apn) => {
            let (path, settings) = gsm_connection_or_new(bus)?;
            update(bus, &path, settings, |s| set(s, "gsm", "apn", value(apn)))?;
            if active_of(bus, &path)?.is_some() {
                deactivate(bus, &path)?;
                activate(bus, &path)?;
            }
            Ok(())
        }
        Cmd::Mode(index) => {
            let (path, _) = modem(bus)?.ok_or(zbus::Error::Failure("The modem is not ready".into()))?;
            let (_, allowed, preferred) = MODES[index];
            Proxy::new(bus, MM, path, MM_MODEM)?.call("SetCurrentModes", &((allowed, preferred),))
        }
    }
}

fn tech_name(access: u32) -> &'static str {
    match access {
        a if a & (1 << 15) != 0 => "5G",
        a if a & (1 << 14) != 0 => "LTE",
        a if a & (1 << 9) != 0 => "HSPA+",
        a if a & (0x7 << 6) != 0 => "HSPA",
        a if a & (1 << 5) != 0 => "UMTS",
        a if a & (1 << 4) != 0 => "EDGE",
        a if a & (1 << 3) != 0 => "GPRS",
        a if a & 0x6 != 0 => "GSM",
        _ => "",
    }
}

fn status_text(state: i32, failed_reason: u32) -> &'static str {
    match state {
        -1 if failed_reason == 2 => "No SIM",
        -1 => "Modem error",
        0 | 1 => "Starting…",
        2 => "SIM locked",
        3 | 4 => "Off",
        5 => "Turning on…",
        6 | 7 => "Searching…",
        8 => "Registered",
        9 => "Disconnecting…",
        10 => "Connecting…",
        11 => "Connected",
        _ => "Unknown",
    }
}

fn read_state(bus: &Connection) -> zbus::Result<State> {
    let mut state = State::default();
    if let Some((_, settings)) = gsm_connection(bus)? {
        let get = |group: &str, key: &str| settings.get(group).and_then(|g| g.get(key)).and_then(|v| v.try_clone().ok());
        state.data_enabled = get("connection", "autoconnect").and_then(|v| bool::try_from(v).ok()).unwrap_or(true);
        state.roaming = !get("gsm", "home-only").and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
        state.apn = get("gsm", "apn").and_then(|v| String::try_from(v).ok()).unwrap_or_default();
    } else {
        state.roaming = true;
        state.apn = DEFAULT_APN.into();
    }

    let Some((_, ifaces)) = modem(bus)? else { return Ok(state) };
    let modem = &ifaces[MM_MODEM];
    let get = |key: &str| modem.get(key).and_then(|v| v.try_clone().ok());
    let mm_state = get("State").and_then(|v| i32::try_from(v).ok()).unwrap_or(0);
    let reason = get("StateFailedReason").and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    let unlock = get("UnlockRequired").and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    let (signal, _): (u32, bool) = get("SignalQuality").and_then(|v| v.try_into().ok()).unwrap_or((0, false));
    let access = get("AccessTechnologies").and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    let current: (u32, u32) = get("CurrentModes").and_then(|v| v.try_into().ok()).unwrap_or((0, 0));
    let supported: Vec<(u32, u32)> = get("SupportedModes").and_then(|v| v.try_into().ok()).unwrap_or_default();

    state.available = true;
    state.status = status_text(mm_state, reason).into();
    state.data_active = mm_state == 11;
    state.signal = signal.min(100);
    state.tech = if mm_state >= 8 { tech_name(access).into() } else { String::new() };
    state.imei = get("EquipmentIdentifier").and_then(|v| String::try_from(v).ok()).unwrap_or_default();
    state.sim = match (mm_state, reason, unlock) {
        (-1, 2, _) => "Missing".into(),
        (_, _, u) if u > 1 => "Locked".into(),
        (s, _, _) if s >= 3 => "Ready".into(),
        _ => "Unknown".into(),
    };
    state.operator = ifaces
        .get(MM_3GPP)
        .and_then(|m| m.get("OperatorName"))
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_default();
    state.modes = (0..MODES.len())
        .filter(|&i| supported.iter().any(|&(allowed, _)| allowed & MODES[i].1 == MODES[i].1))
        .collect();
    state.mode = MODES.iter().position(|&(_, allowed, preferred)| (allowed, preferred) == current);
    Ok(state)
}
