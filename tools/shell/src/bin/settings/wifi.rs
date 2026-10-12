use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use calloop::channel::Sender;
use zbus::blocking::{Connection, Proxy, fdo::ObjectManagerProxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const IWD: &str = "net.connman.iwd";
const AGENT: &str = "/agm/settings/agent";

pub enum Cmd {
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
        std::thread::spawn(move || run(rx, events));
        Wifi { tx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

struct Agent {
    passphrase: Arc<Mutex<Option<String>>>,
}

#[zbus::interface(name = "net.connman.iwd.Agent")]
impl Agent {
    fn release(&self) {}

    fn request_passphrase(&self, _network: OwnedObjectPath) -> zbus::fdo::Result<String> {
        self.passphrase
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| zbus::fdo::Error::Failed("no passphrase".into()))
    }

    fn cancel(&self, _reason: String) {}
}

fn run(rx: mpsc::Receiver<Cmd>, events: Sender<Event>) {
    let bus = match Connection::system() {
        Ok(bus) => bus,
        Err(e) => {
            let _ = events.send(Event::Message(format!("No system bus: {e}")));
            return;
        }
    };
    let passphrase = Arc::new(Mutex::new(None));
    let _ = bus.object_server().at(AGENT, Agent { passphrase: passphrase.clone() });
    let _ = Proxy::new(&bus, IWD, "/net/connman/iwd", "net.connman.iwd.AgentManager")
        .and_then(|p| p.call::<_, _, ()>("RegisterAgent", &(ObjectPath::try_from(AGENT).unwrap(),)));

    let mut state = State::default();
    loop {
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(cmd) => {
                if let Err(e) = handle(&bus, &state, &passphrase, cmd) {
                    let _ = events.send(Event::Message(describe(&e)));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        state = read_state(&bus).unwrap_or_default();
        if events.send(Event::State(state.clone())).is_err() {
            return;
        }
    }
}

fn handle(bus: &Connection, state: &State, passphrase: &Mutex<Option<String>>, cmd: Cmd) -> zbus::Result<()> {
    let device = state.device.clone().ok_or(zbus::Error::Failure("no Wi-Fi device".into()))?;
    let station = || Proxy::new(bus, IWD, device.clone(), "net.connman.iwd.Station");
    match cmd {
        Cmd::Power(on) => {
            let props = Proxy::new(bus, IWD, device.clone(), "org.freedesktop.DBus.Properties")?;
            props.call::<_, _, ()>("Set", &("net.connman.iwd.Device", "Powered", Value::from(on)))
        }
        Cmd::Scan => station()?.call("Scan", &()),
        Cmd::Connect { network, passphrase: pass } => {
            *passphrase.lock().unwrap() = pass;
            let result = Proxy::new(bus, IWD, network, "net.connman.iwd.Network")?.call("Connect", &());
            *passphrase.lock().unwrap() = None;
            result
        }
        Cmd::Disconnect => station()?.call("Disconnect", &()),
        Cmd::Forget(known) => Proxy::new(bus, IWD, known, "net.connman.iwd.KnownNetwork")?.call("Forget", &()),
    }
}

fn describe(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, desc, _) => {
            let short = name.as_str().rsplit('.').next().unwrap_or("Error");
            match (short, desc.as_deref()) {
                ("Failed", _) => "Could not connect. Wrong password?".into(),
                ("Aborted", _) => "Cancelled".into(),
                ("Busy", _) | ("InProgress", _) => "Wi-Fi is busy, try again".into(),
                (_, Some(desc)) => desc.to_string(),
                (short, None) => short.to_string(),
            }
        }
        e => e.to_string(),
    }
}

type Props = HashMap<String, OwnedValue>;

fn get<T: TryFrom<OwnedValue>>(props: Option<&Props>, key: &str) -> Option<T> {
    T::try_from(props?.get(key)?.try_clone().ok()?).ok()
}

fn read_state(bus: &Connection) -> zbus::Result<State> {
    let objects = ObjectManagerProxy::builder(bus).destination(IWD)?.path("/")?.build()?.get_managed_objects()?;
    let Some((device, ifaces)) = objects.iter().find(|(_, i)| i.contains_key("net.connman.iwd.Device")) else {
        return Ok(State::default());
    };
    let station = ifaces.get("net.connman.iwd.Station");
    let mut state = State {
        available: true,
        powered: get(ifaces.get("net.connman.iwd.Device"), "Powered").unwrap_or(false),
        scanning: get(station, "Scanning").unwrap_or(false),
        state: get(station, "State").unwrap_or_default(),
        networks: Vec::new(),
        device: Some(device.clone()),
    };
    if station.is_none() {
        return Ok(state);
    }

    let proxy = Proxy::new(bus, IWD, device.as_ref(), "net.connman.iwd.Station")?;
    let ordered: Vec<(OwnedObjectPath, i16)> = proxy.call("GetOrderedNetworks", &())?;
    for (path, rssi) in ordered {
        let props = objects.get(&path).and_then(|i| i.get("net.connman.iwd.Network"));
        state.networks.push(Network {
            name: get(props, "Name").unwrap_or_default(),
            kind: get(props, "Type").unwrap_or_default(),
            known: get(props, "KnownNetwork"),
            connected: get(props, "Connected").unwrap_or(false),
            bars: agm_shell::status::bars(rssi),
            path,
        });
    }
    Ok(state)
}
