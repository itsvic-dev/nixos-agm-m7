mod system;
mod ui;
mod wifi;

use std::time::{Duration, Instant};

use agm_shell::config::{self, Config};
use agm_shell::{pids, shell::Shell, wl::present};
use chrono::Local;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    dispatch2::Dispatch2,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            EventLoop, LoopHandle,
            channel::{self, Event},
            timer::{TimeoutAction, Timer},
        },
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, QueueHandle,
            globals::registry_queue_init,
            protocol::{wl_keyboard, wl_output, wl_seat, wl_surface, wl_touch},
        },
        protocols::wp::text_input::zv3::client::{
            zwp_text_input_manager_v3::ZwpTextInputManagerV3,
            zwp_text_input_v3::{self, ContentHint, ContentPurpose, ZwpTextInputV3},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        touch::TouchHandler,
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use tiny_skia::Pixmap;
use ui::{Kind, Row};

const KEY_BACKSPACE: u32 = 14;
const KEY_ENTER: u32 = 28;
const KEY_UP: u32 = 103;
const KEY_LEFT: u32 = 105;
const KEY_RIGHT: u32 = 106;
const KEY_DOWN: u32 = 108;
const KEY_BACK: u32 = 158;

const BACKLIGHT: &str = "/sys/class/backlight/backlight";
const DIM_CHOICES: [u32; 5] = [10, 20, 30, 60, 0];
const OFF_CHOICES: [u32; 6] = [15, 30, 60, 120, 300, 0];
const TIMEDATE: &str = "org.freedesktop.timedate1";

enum Page {
    Root,
    Wifi,
    Network(wifi::Network),
    Password { network: wifi::Network, text: String, preedit: String, show: bool },
    Display,
    Battery,
    DateTime,
    Regions(Vec<String>),
    Cities(Vec<String>),
    Storage,
    About,
}

struct Frame {
    page: Page,
    selected: usize,
    scroll: usize,
}

#[derive(Default)]
struct Pending {
    commit: Option<String>,
    preedit: Option<String>,
    delete: u32,
}

struct App {
    conn: Connection,
    handle: LoopHandle<'static, App>,
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    size: (u32, u32),
    configured: bool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    touched: Option<(i32, usize)>,
    ti_manager: Option<ZwpTextInputManagerV3>,
    text_input: Option<ZwpTextInputV3>,
    ti_entered: bool,
    ti_enabled: bool,
    pending: Pending,
    shell: Shell,
    stack: Vec<Frame>,
    wifi: wifi::Wifi,
    wifi_state: wifi::State,
    config: Config,
    bus: Option<zbus::blocking::Connection>,
    message: Option<(String, Instant)>,
}

fn timedate(bus: &zbus::blocking::Connection) -> zbus::Result<zbus::blocking::Proxy<'_>> {
    zbus::blocking::Proxy::new(bus, TIMEDATE, "/org/freedesktop/timedate1", TIMEDATE)
}

fn seconds(s: u32) -> String {
    match s {
        0 => "Never".into(),
        s if s < 60 => format!("{s} seconds"),
        60 => "1 minute".into(),
        s => format!("{} minutes", s / 60),
    }
}

fn read_u32(path: &str) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn brightness() -> f32 {
    let cur = read_u32(&format!("{BACKLIGHT}/brightness")).unwrap_or(0) as f32;
    let max = read_u32(&format!("{BACKLIGHT}/max_brightness")).unwrap_or(1).max(1) as f32;
    cur / max
}

fn set_brightness(f: f32) {
    let max = read_u32(&format!("{BACKLIGHT}/max_brightness")).unwrap_or(1) as f32;
    let _ = std::fs::write(format!("{BACKLIGHT}/brightness"), ((max * f).round() as u32).to_string());
}

impl App {
    fn top(&self) -> &Frame {
        self.stack.last().unwrap()
    }

    fn top_mut(&mut self) -> &mut Frame {
        self.stack.last_mut().unwrap()
    }

    fn say(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), Instant::now()));
        self.handle
            .insert_source(Timer::from_duration(Duration::from_millis(4100)), |_, _, app| {
                app.draw();
                TimeoutAction::Drop
            })
            .unwrap();
        self.draw();
    }

    fn timezone(&self) -> String {
        self.bus
            .as_ref()
            .and_then(|b| timedate(b).ok()?.get_property::<String>("Timezone").ok())
            .unwrap_or_else(|| "Unknown".into())
    }

    fn title(&self) -> String {
        match &self.top().page {
            Page::Root => "Settings".into(),
            Page::Wifi => "Wi-Fi".into(),
            Page::Network(n) | Page::Password { network: n, .. } => n.name.clone(),
            Page::Display => "Display".into(),
            Page::Battery => "Battery".into(),
            Page::DateTime => "Date & time".into(),
            Page::Regions(_) => "Time zone".into(),
            Page::Cities(zones) => zones.first().and_then(|z| z.split('/').next()).unwrap_or("Time zone").into(),
            Page::Storage => "Storage".into(),
            Page::About => "About phone".into(),
        }
    }

    fn rows(&self) -> Vec<Row> {
        let w = &self.wifi_state;
        let info = |rows: Vec<(String, String)>| rows.into_iter().map(|(k, v)| Row::new(k, v, Kind::Info)).collect();
        match &self.top().page {
            Page::Root => {
                let wifi = if !w.available {
                    "Not available".to_string()
                } else if !w.powered {
                    "Off".into()
                } else if w.state == "connecting" {
                    "Connecting…".into()
                } else {
                    w.networks.iter().find(|n| n.connected).map_or("Not connected".into(), |n| n.name.clone())
                };
                let battery = system::battery().into_iter().take(2).map(|(_, v)| v).collect::<Vec<_>>().join(", ");
                let storage = system::storage()
                    .first()
                    .map_or(String::new(), |(_, used, total)| format!("{} of {} used", system::size(*used), system::size(*total)));
                let status = agm_shell::status::Status { battery: agm_shell::status::battery(), wifi: None };
                vec![
                    Row::new("Wi-Fi", wifi, Kind::Link).icon("network-wireless-symbolic"),
                    Row::new("Display", format!("Brightness {:.0}%", brightness() * 100.0), Kind::Link).icon("display-brightness-symbolic"),
                    Row::new("Battery", battery, Kind::Link).icon(agm_shell::shell::battery_icon(&status)),
                    Row::new("Date & time", self.timezone(), Kind::Link).icon("preferences-system-time-symbolic"),
                    Row::new("Storage", storage, Kind::Link).icon("drive-harddisk-symbolic"),
                    Row::new("About phone", system::model().unwrap_or_default(), Kind::Link).icon("help-about-symbolic"),
                ]
            }
            Page::Wifi => {
                let mut rows = vec![Row::new("Wi-Fi", "", Kind::Toggle(w.powered))];
                if w.powered {
                    let scan = if w.scanning { "Scanning…" } else { "Scan for networks" };
                    rows.push(Row::new(scan, "", Kind::Button));
                    for n in &w.networks {
                        let status = match (n.connected, n.known.is_some(), n.kind.as_str()) {
                            (true, _, _) => "Connected",
                            (_, true, _) => "Saved",
                            (_, _, "open") => "Open",
                            (_, _, "8021x") => "Enterprise",
                            _ => "Secured",
                        };
                        rows.push(Row::new(&n.name, status, Kind::Network { bars: n.bars, locked: n.kind != "open" }));
                    }
                }
                rows
            }
            Page::Network(n) => {
                let mut rows = Vec::new();
                if n.connected {
                    rows.push(Row::new("Disconnect", "", Kind::Button));
                } else {
                    rows.push(Row::new("Connect", "", Kind::Button));
                }
                if n.known.is_some() {
                    rows.push(Row::new("Forget network", "", Kind::Button));
                }
                rows
            }
            Page::Password { text, preedit, show, .. } => {
                let shown = if *show { text.clone() } else { "•".repeat(text.chars().count()) };
                vec![
                    Row::new("Password", "", Kind::Field { text: shown, preedit: preedit.clone(), focused: self.top().selected == 0 }),
                    Row::new("Show password", "", Kind::Toggle(*show)),
                    Row::new("Connect", "", Kind::Button),
                ]
            }
            Page::Display => {
                let dim = self.config.get_u32(config::DIM_AFTER, config::DEFAULT_DIM_AFTER);
                let off = self.config.get_u32(config::OFF_AFTER, config::DEFAULT_OFF_AFTER);
                let b = brightness();
                vec![
                    Row::new("Brightness", format!("{:.0}%", b * 100.0), Kind::Slider(b)),
                    Row::new("Dim after", seconds(dim), Kind::Info),
                    Row::new("Screen off and lock after", seconds(off), Kind::Info),
                ]
            }
            Page::Battery => info(system::battery()),
            Page::DateTime => {
                let h24 = self.config.get_bool(config::CLOCK_24H, true);
                let td = self.bus.as_ref().and_then(|b| timedate(b).ok());
                let ntp = td.as_ref().and_then(|p| p.get_property::<bool>("NTP").ok()).unwrap_or(false);
                let synced = td.as_ref().and_then(|p| p.get_property::<bool>("NTPSynchronized").ok()).unwrap_or(false);
                let auto = match (ntp, synced) {
                    (false, _) => "Off",
                    (true, true) => "On, synchronized",
                    (true, false) => "On, not synchronized yet",
                };
                let now = Local::now().format(if h24 { "%H:%M, %-d %B %Y" } else { "%-I:%M %p, %-d %B %Y" });
                vec![
                    Row::new("Time zone", self.timezone(), Kind::Link),
                    Row::new("24-hour clock", "", Kind::Toggle(h24)),
                    Row::new("Automatic time", auto, Kind::Info),
                    Row::new("Time", now.to_string(), Kind::Info),
                ]
            }
            Page::Regions(regions) => regions.iter().map(|r| Row::new(r.replace('_', " "), "", Kind::Link)).collect(),
            Page::Cities(zones) => zones
                .iter()
                .map(|z| Row::new(z.split_once('/').map_or(z.as_str(), |(_, c)| c).replace('_', " ").replace('/', " / "), "", Kind::Button))
                .collect(),
            Page::Storage => system::storage()
                .into_iter()
                .map(|(name, used, total)| {
                    let value = format!("{} of {}", system::size(used), system::size(total));
                    Row::new(name, value, Kind::Bar(used as f32 / total.max(1) as f32))
                })
                .collect(),
            Page::About => info(system::about()),
        }
    }

    fn draw(&mut self) {
        if !self.configured {
            return;
        }
        let (w, h) = self.size;
        let Some(mut pm) = Pixmap::new(w, h) else { return };
        let rows = self.rows();
        let title = self.title();
        if let Some((_, at)) = &self.message {
            if at.elapsed() > Duration::from_secs(4) {
                self.message = None;
            }
        }
        let frame = self.top();
        let view = ui::View {
            title: &title,
            rows: &rows,
            selected: frame.selected.min(rows.len().saturating_sub(1)),
            scroll: frame.scroll,
            message: self.message.as_ref().map(|(m, _)| m.as_str()),
            right: if self.stack.len() > 1 { "Back" } else { "Close" },
        };
        ui::draw(&mut self.shell, &mut pm, &view);
        present(&mut self.pool, self.window.wl_surface(), &pm);
    }

    fn select(&mut self, index: usize) {
        let count = self.rows().len();
        let visible = ui::visible_rows(self.size.1);
        let frame = self.top_mut();
        frame.selected = index.min(count.saturating_sub(1));
        if frame.selected < frame.scroll {
            frame.scroll = frame.selected;
        } else if frame.selected >= frame.scroll + visible {
            frame.scroll = frame.selected + 1 - visible;
        }
        self.update_text_input();
        self.draw();
    }

    fn push(&mut self, page: Page) {
        self.stack.push(Frame { page, selected: 0, scroll: 0 });
        self.update_text_input();
        self.draw();
    }

    fn back(&mut self) {
        if self.stack.len() == 1 {
            std::process::exit(0);
        }
        self.stack.pop();
        self.update_text_input();
        self.draw();
    }

    fn update_text_input(&mut self) {
        let want = matches!(self.top().page, Page::Password { .. }) && self.top().selected == 0 && self.ti_entered;
        let Some(ti) = &self.text_input else { return };
        if want && !self.ti_enabled {
            ti.enable();
            ti.set_content_type(ContentHint::SensitiveData | ContentHint::Latin, ContentPurpose::Password);
            ti.set_cursor_rectangle(10, ui::HEADER as i32 + 4, self.size.0 as i32 - 20, ui::ROW as i32 - 8);
            ti.commit();
        } else if !want && self.ti_enabled {
            ti.disable();
            ti.commit();
        }
        self.ti_enabled = want;
    }

    fn field(&mut self) -> Option<(&mut String, &mut String)> {
        let frame = self.stack.last_mut()?;
        match &mut frame.page {
            Page::Password { text, preedit, .. } if frame.selected == 0 => Some((text, preedit)),
            _ => None,
        }
    }

    fn connect_password(&mut self) {
        let Page::Password { network, text, .. } = &self.top().page else { return };
        if text.chars().count() < 8 {
            self.say("A Wi-Fi password has at least 8 characters");
            return;
        }
        let (network, text) = (network.clone(), text.clone());
        self.wifi.send(wifi::Cmd::Connect { network: network.path, passphrase: Some(text) });
        self.stack.pop();
        self.update_text_input();
        self.say(format!("Connecting to {}…", network.name));
    }

    fn activate(&mut self, index: usize) {
        let rows = self.rows().len();
        if index >= rows {
            return;
        }
        match &self.top().page {
            Page::Root => match index {
                0 => self.push(Page::Wifi),
                1 => self.push(Page::Display),
                2 => self.push(Page::Battery),
                3 => self.push(Page::DateTime),
                4 => self.push(Page::Storage),
                _ => self.push(Page::About),
            },
            Page::Wifi => match index {
                0 => {
                    let on = !self.wifi_state.powered;
                    self.wifi.send(wifi::Cmd::Power(on));
                    self.say(if on { "Turning Wi-Fi on…" } else { "Turning Wi-Fi off…" });
                }
                1 => self.wifi.send(wifi::Cmd::Scan),
                i => {
                    let Some(n) = self.wifi_state.networks.get(i - 2).cloned() else { return };
                    if n.connected || n.known.is_some() {
                        self.push(Page::Network(n));
                    } else if n.kind == "open" {
                        self.wifi.send(wifi::Cmd::Connect { network: n.path.clone(), passphrase: None });
                        self.say(format!("Connecting to {}…", n.name));
                    } else if n.kind == "psk" {
                        self.push(Page::Password { network: n, text: String::new(), preedit: String::new(), show: false });
                    } else {
                        self.say("Enterprise networks are not supported yet");
                    }
                }
            },
            Page::Network(n) => {
                let n = n.clone();
                let label = self.rows()[index].title.clone();
                match label.as_str() {
                    "Disconnect" => self.wifi.send(wifi::Cmd::Disconnect),
                    "Connect" => self.wifi.send(wifi::Cmd::Connect { network: n.path.clone(), passphrase: None }),
                    _ => {
                        if let Some(known) = n.known.clone() {
                            self.wifi.send(wifi::Cmd::Forget(known));
                        }
                    }
                }
                self.stack.pop();
                self.say(format!("{label}: {}", n.name));
            }
            Page::Password { .. } => match index {
                0 => self.connect_password(),
                1 => {
                    if let Page::Password { show, .. } = &mut self.top_mut().page {
                        *show = !*show;
                    }
                    self.draw();
                }
                _ => self.connect_password(),
            },
            Page::Display => self.adjust(index, 1),
            Page::DateTime => match index {
                0 => {
                    let zones: Vec<String> = self
                        .bus
                        .as_ref()
                        .and_then(|b| timedate(b).ok()?.call("ListTimezones", &()).ok())
                        .unwrap_or_default();
                    let mut regions: Vec<String> = zones.iter().filter_map(|z| z.split_once('/').map(|(r, _)| r.to_string())).collect();
                    regions.sort();
                    regions.dedup();
                    regions.push("UTC".into());
                    self.push(Page::Regions(regions));
                }
                1 => {
                    let h24 = self.config.get_bool(config::CLOCK_24H, true);
                    self.config.set(config::CLOCK_24H, !h24);
                    let _ = self.config.save();
                    self.draw();
                }
                _ => {}
            },
            Page::Regions(regions) => {
                let region = regions[index].clone();
                if region == "UTC" {
                    self.set_timezone("UTC".into(), 1);
                    return;
                }
                let zones: Vec<String> = self
                    .bus
                    .as_ref()
                    .and_then(|b| timedate(b).ok()?.call::<_, _, Vec<String>>("ListTimezones", &()).ok())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|z| z.starts_with(&format!("{region}/")))
                    .collect();
                self.push(Page::Cities(zones));
            }
            Page::Cities(zones) => {
                let zone = zones[index].clone();
                self.set_timezone(zone, 2);
            }
            Page::Battery | Page::Storage | Page::About => {}
        }
    }

    fn set_timezone(&mut self, zone: String, pop: usize) {
        let result = self
            .bus
            .as_ref()
            .ok_or(zbus::Error::Failure("no system bus".into()))
            .and_then(|b| timedate(b)?.call::<_, _, ()>("SetTimezone", &(zone.as_str(), false)));
        for _ in 0..pop {
            self.stack.pop();
        }
        match result {
            Ok(()) => self.say(format!("Time zone: {zone}")),
            Err(e) => self.say(format!("Could not set the time zone: {e}")),
        }
    }

    fn adjust(&mut self, index: usize, delta: i32) {
        match &self.top().page {
            Page::Display => {}
            Page::Wifi if index == 0 => {
                if (delta > 0) != self.wifi_state.powered {
                    self.activate(0);
                }
                return;
            }
            Page::DateTime | Page::Password { .. } if self.rows().get(index).is_some_and(|r| matches!(r.kind, Kind::Toggle(_))) => {
                self.activate(index);
                return;
            }
            _ => return,
        }
        let step = |choices: &[u32], current: u32| {
            let i = choices.iter().position(|&c| c == current).unwrap_or(0) as i32;
            choices[(i + delta).rem_euclid(choices.len() as i32) as usize]
        };
        let mut dim = self.config.get_u32(config::DIM_AFTER, config::DEFAULT_DIM_AFTER);
        let mut off = self.config.get_u32(config::OFF_AFTER, config::DEFAULT_OFF_AFTER);
        match index {
            0 => {
                let b = (brightness() + 0.1 * delta as f32).clamp(0.05, 1.0);
                set_brightness(b);
                self.draw();
                return;
            }
            1 => {
                dim = step(&DIM_CHOICES, dim);
                if dim != 0 && off != 0 && off <= dim {
                    off = OFF_CHOICES.iter().copied().find(|&o| o > dim).unwrap_or(0);
                }
            }
            _ => {
                off = step(&OFF_CHOICES, off);
                if dim != 0 && off != 0 && dim >= off {
                    dim = DIM_CHOICES.iter().copied().filter(|&d| d != 0 && d < off).max().unwrap_or(0);
                }
            }
        }
        self.config.set(config::DIM_AFTER, dim);
        self.config.set(config::OFF_AFTER, off);
        let _ = self.config.save();
        // The idle script restarts swayidle with the new delays.
        for pid in pids("swayidle") {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
        }
        self.draw();
    }

    fn key(&mut self, event: &KeyEvent) {
        let selected = self.top().selected;
        match event.raw_code {
            KEY_UP => self.select(selected.saturating_sub(1)),
            KEY_DOWN => self.select(selected + 1),
            KEY_LEFT => self.adjust(selected, -1),
            KEY_RIGHT => self.adjust(selected, 1),
            KEY_ENTER => self.activate(selected),
            KEY_BACK => self.back(),
            KEY_BACKSPACE => match self.field() {
                Some((text, _)) if !text.is_empty() => {
                    text.pop();
                    self.draw();
                }
                _ => self.back(),
            },
            _ => {
                let typed = event.utf8.as_deref().filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
                if let (Some(typed), Some((text, _))) = (typed.map(String::from), self.field()) {
                    text.push_str(&typed);
                    self.draw();
                }
            }
        }
    }

    fn text_input_done(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        let Some((text, preedit)) = self.field() else { return };
        let mut removed = 0;
        while removed < pending.delete as usize {
            let Some(c) = text.pop() else { break };
            removed += c.len_utf8();
        }
        if let Some(commit) = pending.commit {
            text.push_str(&commit);
        }
        *preedit = pending.preedit.unwrap_or_default();
        self.draw();
    }
}

struct TextInputData;

impl Dispatch2<ZwpTextInputManagerV3, App> for TextInputData {
    fn event(&self, _: &mut App, _: &ZwpTextInputManagerV3, _: <ZwpTextInputManagerV3 as smithay_client_toolkit::reexports::client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
}

impl Dispatch2<ZwpTextInputV3, App> for TextInputData {
    fn event(&self, app: &mut App, _: &ZwpTextInputV3, event: zwp_text_input_v3::Event, _: &Connection, _: &QueueHandle<App>) {
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                app.ti_entered = true;
                app.ti_enabled = false;
                app.update_text_input();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                app.ti_entered = false;
                app.ti_enabled = false;
            }
            zwp_text_input_v3::Event::PreeditString { text, .. } => app.pending.preedit = text,
            zwp_text_input_v3::Event::CommitString { text } => app.pending.commit = text,
            zwp_text_input_v3::Event::DeleteSurroundingText { before_length, .. } => app.pending.delete = before_length,
            zwp_text_input_v3::Event::Done { .. } => app.text_input_done(),
            _ => {}
        }
    }
}

fn main() {
    if pids("agm-settings").into_iter().any(|p| p != std::process::id()) {
        let _ = std::process::Command::new(env!("AGM_SWAYMSG")).arg("[app_id=agm-settings] focus").output();
        return;
    }

    let conn = Connection::connect_to_env().expect("no Wayland display");
    let (globals, queue) = registry_queue_init(&conn).unwrap();
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let xdg = XdgShell::bind(&globals, &qh).expect("xdg_shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let ti_manager = globals.bind::<ZwpTextInputManagerV3, _, _>(&qh, 1..=1, TextInputData).ok();

    let window = xdg.create_window(compositor.create_surface(&qh), WindowDecorations::ServerDefault, &qh);
    window.set_title("Settings");
    window.set_app_id("agm-settings");
    window.commit();

    let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
    let (wifi_tx, wifi_rx) = channel::channel();
    let mut app = App {
        conn: conn.clone(),
        handle: event_loop.handle(),
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        pool: SlotPool::new(240 * 320 * 4, &shm).expect("shm pool"),
        shm,
        window,
        size: (240, 306),
        configured: false,
        keyboard: None,
        touch: None,
        touched: None,
        ti_manager,
        text_input: None,
        ti_entered: false,
        ti_enabled: false,
        pending: Pending::default(),
        shell: Shell::load(),
        stack: vec![Frame { page: Page::Root, selected: 0, scroll: 0 }],
        wifi: wifi::Wifi::start(wifi_tx),
        wifi_state: wifi::State::default(),
        config: Config::load(),
        bus: zbus::blocking::Connection::system().ok(),
        message: None,
    };

    let handle = event_loop.handle();
    WaylandSource::new(conn, queue).insert(handle.clone()).unwrap();
    handle
        .insert_source(wifi_rx, |event, _, app| match event {
            Event::Msg(wifi::Event::State(state)) => {
                app.wifi_state = state;
                if matches!(app.top().page, Page::Root | Page::Wifi) {
                    app.draw();
                }
            }
            Event::Msg(wifi::Event::Message(text)) => app.say(text),
            Event::Closed => {}
        })
        .unwrap();
    handle
        .insert_source(Timer::from_duration(Duration::from_secs(5)), |_, _, app| {
            if matches!(app.top().page, Page::Root | Page::Battery | Page::DateTime | Page::About | Page::Display) {
                app.draw();
            }
            TimeoutAction::ToDuration(Duration::from_secs(5))
        })
        .unwrap();

    loop {
        event_loop.dispatch(None, &mut app).unwrap();
        let _ = app.conn.flush();
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        std::process::exit(0);
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, configure: WindowConfigure, _: u32) {
        let (w, h) = configure.new_size;
        self.size = (w.map_or(240, |w| w.get()), h.map_or(306, |h| h.get()));
        self.configured = true;
        self.draw();
    }
}

impl KeyboardHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        if matches!(event.raw_code, KEY_UP | KEY_DOWN | KEY_LEFT | KEY_RIGHT | KEY_BACKSPACE) {
            self.key(&event);
        }
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: Modifiers, _: RawModifiers, _: u32) {}
}

impl TouchHandler for App {
    fn down(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, _: u32, _: wl_surface::WlSurface, id: i32, (x, y): (f64, f64)) {
        let (w, h) = self.size;
        if y as f32 > h as f32 - agm_shell::shell::SOFT_KEY_BAR {
            if x as f32 > w as f32 / 2.0 {
                self.back();
            }
            return;
        }
        if let Some(row) = ui::row_at(h, self.top().scroll, y as f32).filter(|&r| r < self.rows().len()) {
            self.touched = Some((id, row));
            self.select(row);
        }
    }
    fn up(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, _: u32, id: i32) {
        if let Some((touch, row)) = self.touched.take() {
            if touch == id {
                self.activate(row);
            }
        }
    }
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, _: i32, _: (f64, f64)) {}
    fn shape(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: i32, _: f64, _: f64) {}
    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: i32, _: f64) {}
    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) {
        self.touched = None;
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
            if let Some(manager) = &self.ti_manager {
                self.text_input = Some(manager.get_text_input(&seat, qh, TextInputData));
            }
        }
        if capability == Capability::Touch && self.touch.is_none() {
            self.touch = self.seats.get_touch(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }
        if capability == Capability::Touch {
            if let Some(touch) = self.touch.take() {
                touch.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_registry!(App);
smithay_client_toolkit::delegate_dispatch2!(App);
