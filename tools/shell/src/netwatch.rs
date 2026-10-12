use std::sync::mpsc;
use std::time::Duration;

use zbus::blocking::{Connection, MessageIterator, Proxy, fdo::DBusProxy, fdo::ObjectManagerProxy};
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use zbus::MatchRule;

const IWD: &str = "net.connman.iwd";
const SENDERS: [&str; 3] = ["org.freedesktop.NetworkManager", "org.freedesktop.ModemManager1", IWD];
/// Signal level thresholds in dBm; they match `status::bars`.
const LEVELS: [i16; 4] = [-60, -67, -74, -80];

struct SignalAgent;

#[zbus::interface(name = "net.connman.iwd.SignalLevelAgent")]
impl SignalAgent {
    fn release(&self, _device: OwnedObjectPath) {}
    fn changed(&self, _device: OwnedObjectPath, _level: u8) {}
}

/// Calls `notify` after network state changes in NetworkManager, ModemManager or iwd.
/// Bursts of changes give one call. The watcher stops when `notify` returns false.
pub fn watch(notify: impl Fn() -> bool + Send + 'static) {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(150)).is_ok() {}
            if !notify() {
                return;
            }
        }
    });
    std::thread::spawn(move || {
        let _ = listen(tx);
    });
}

fn listen(tx: mpsc::Sender<()>) -> zbus::Result<()> {
    let bus = Connection::system()?;
    let dbus = DBusProxy::new(&bus)?;
    for sender in SENDERS {
        dbus.add_match_rule(MatchRule::builder().msg_type(Type::Signal).sender(sender)?.build())?;
    }
    let agent = format!("/agm/signal_agent_{}", std::process::id());
    bus.object_server().at(agent.as_str(), SignalAgent)?;
    let mut agent_registered = register_agent(&bus, &agent);

    for msg in MessageIterator::from(&bus) {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let ours = match msg.message_type() {
            Type::Signal => true,
            Type::MethodCall => header.path().is_some_and(|p| p.as_str() == agent),
            _ => false,
        };
        if !ours {
            continue;
        }
        if !agent_registered && header.interface().is_some_and(|i| i.as_str() == "org.freedesktop.DBus.ObjectManager") {
            agent_registered = register_agent(&bus, &agent);
        }
        if tx.send(()).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// iwd reports signal changes of the connected network only to a registered agent.
fn register_agent(bus: &Connection, agent: &str) -> bool {
    let Ok(manager) = ObjectManagerProxy::builder(bus).destination(IWD).and_then(|b| b.path("/")).and_then(|b| b.build())
    else {
        return false;
    };
    let Ok(objects) = manager.get_managed_objects() else { return false };
    let path = ObjectPath::try_from(agent).unwrap();
    objects
        .iter()
        .filter(|(_, ifaces)| ifaces.contains_key("net.connman.iwd.Station"))
        .any(|(station, _)| {
            Proxy::new(bus, IWD, station.as_ref(), "net.connman.iwd.Station")
                .and_then(|p| p.call::<_, _, ()>("RegisterSignalLevelAgent", &(&path, LEVELS.to_vec())))
                .is_ok()
        })
}
