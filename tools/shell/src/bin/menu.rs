use std::collections::HashSet;
use std::os::unix::process::CommandExt;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use agm_shell::draw::{image, rect, ring, rounded};
use agm_shell::icons::data_dirs;
use agm_shell::shell::{SOFT_KEY_BAR, Shell};
use agm_shell::text::Weight;
use agm_shell::theme::{DARK, WHITE};
use agm_shell::{pids, wl::present};
use calloop::signals::{Signal, Signals};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            EventLoop, LoopHandle,
            timer::{TimeoutAction, Timer},
        },
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, QueueHandle,
            globals::registry_queue_init,
            protocol::{wl_keyboard, wl_output, wl_seat, wl_surface, wl_touch},
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
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use tiny_skia::{Pixmap, Rect};

const KEY_ENTER: u32 = 28;
const KEY_UP: u32 = 103;
const KEY_LEFT: u32 = 105;
const KEY_RIGHT: u32 = 106;
const KEY_DOWN: u32 = 108;
const KEY_POWER: u32 = 116;
const KEY_MENU: u32 = 139;
const KEY_BACK: u32 = 158;
const KEY_1: u32 = 2;
const KEY_9: u32 = 10;

const COLS: usize = 3;
const ROWS: usize = 3;
const HEADER: f32 = 30.0;
const ICON: u32 = 40;

struct App {
    name: String,
    exec: String,
    terminal: bool,
    icon: Option<String>,
}

struct Menu {
    handle: LoopHandle<'static, Menu>,
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    size: (u32, u32),
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    touched: Option<(i32, usize)>,
    shell: Shell,
    apps: Vec<App>,
    selected: usize,
    scroll: usize,
    mode: String,
}

impl Menu {
    fn cell(&self, k: usize) -> Rect {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let cw = w / COLS as f32;
        let ch = (h - HEADER - SOFT_KEY_BAR - 4.0) / ROWS as f32;
        let (col, row) = (k % COLS, k / COLS);
        Rect::from_xywh(col as f32 * cw, HEADER + 2.0 + row as f32 * ch, cw, ch).unwrap()
    }

    fn draw(&mut self) {
        let (w, h) = self.size;
        let Some(mut pm) = Pixmap::new(w, h) else { return };
        pm.fill(agm_shell::draw::rgba(DARK.bg, 255));
        let fonts = &self.shell.fonts;
        fonts.draw(&mut pm, Weight::SemiBold, 14.0, 12.0, 21.0, DARK.text, "Apps");

        for k in 0..COLS * ROWS {
            let index = self.scroll * COLS + k;
            let Some(app) = self.apps.get(index) else { break };
            let cell = self.cell(k);
            let cx = cell.left() + cell.width() / 2.0;
            let size = ICON as f32;
            let top = (cell.top() + (cell.height() - size - 16.0) / 2.0).round();
            let left = (cx - size / 2.0).round();
            let selected = index == self.selected;

            if selected {
                ring(&mut pm, rect(left - 4.0, top - 4.0, size + 8.0, size + 8.0), 15.0, 2.0, DARK.accent, DARK.bg);
            }
            let icon = app.icon.as_deref().and_then(|name| self.shell.icons.get(name, ICON, None));
            match icon {
                Some(icon) => image(&mut pm, icon, left, top),
                None => {
                    rounded(&mut pm, rect(left, top, size, size), 12.0, DARK.separator, 255);
                    let letter = app.name.chars().take(1).collect::<String>().to_uppercase();
                    self.shell.fonts.draw_centered(&mut pm, Weight::SemiBold, 18.0, cx, top + 27.0, WHITE, &letter);
                }
            }

            let fonts = &self.shell.fonts;
            let color = if selected { DARK.accent } else { DARK.text };
            let name = fonts.fit(Weight::Medium, 10.5, &app.name, cell.width() - 6.0);
            fonts.draw_centered(&mut pm, Weight::Medium, 10.5, cx, top + size + 15.0, color, &name);
        }

        self.shell.soft_keys(&mut pm, &DARK, "Open", "Back");
        present(&mut self.pool, self.layer.wl_surface(), &pm);
    }

    fn select(&mut self, index: usize) {
        if self.apps.is_empty() {
            return;
        }
        self.selected = index.min(self.apps.len() - 1);
        let row = self.selected / COLS;
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + ROWS {
            self.scroll = row + 1 - ROWS;
        }
        self.draw();
    }

    fn key(&mut self, code: u32) {
        let s = self.selected;
        match code {
            KEY_LEFT => self.select(s.saturating_sub(1)),
            KEY_RIGHT => self.select(s + 1),
            KEY_UP => self.select(s.saturating_sub(COLS)),
            KEY_DOWN => self.select(s + COLS),
            KEY_ENTER | KEY_MENU => self.launch(s),
            KEY_1..=KEY_9 => {
                let index = self.scroll * COLS + (code - KEY_1) as usize;
                if index < self.apps.len() {
                    self.launch(index);
                }
            }
            KEY_BACK | KEY_POWER => self.close(),
            _ => {}
        }
    }

    fn launch(&mut self, index: usize) {
        let Some(app) = self.apps.get(index) else { return };
        let cmd = if app.terminal {
            format!("{} -e {}", env!("AGM_TERMINAL"), app.exec)
        } else {
            app.exec.clone()
        };
        let _ = Command::new("sh").arg("-c").arg(cmd).process_group(0).spawn();
        self.close();
    }

    fn set_mode(&self, mode: &str) {
        let _ = Command::new(env!("AGM_SWAYMSG")).args(["mode", mode]).output();
    }

    fn close(&self) -> ! {
        self.set_mode(&self.mode);
        std::process::exit(0);
    }
}

fn current_mode() -> String {
    let out = Command::new(env!("AGM_SWAYMSG")).args(["-t", "get_binding_state"]).output();
    let json = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let mode = json.split('"').skip_while(|s| *s != "name").nth(2).unwrap_or("default");
    if mode == "menu" { "default".into() } else { mode.into() }
}

fn load_apps(hidden: &HashSet<String>, names: &HashMap<String, String>) -> Vec<App> {
    let dirs = data_dirs();
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir.join("applications")) else { continue };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let Some(id) = path.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
            if !id.ends_with(".desktop") || !seen.insert(id.clone()) || hidden.contains(&id) {
                continue;
            }
            if let Some(mut app) = parse_desktop(&path) {
                if let Some(name) = names.get(&id) {
                    app.name = name.clone();
                }
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn parse_desktop(path: &Path) -> Option<App> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut get = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry && let Some((k, v)) = line.split_once('=') {
            get.entry(k.trim().to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    let yes = |k: &str| get.get(k).is_some_and(|v| v == "true");
    if get.get("Type").map(String::as_str) != Some("Application") || yes("NoDisplay") || yes("Hidden") {
        return None;
    }
    let exec = get
        .get("Exec")?
        .split(' ')
        .filter(|arg| !(arg.len() == 2 && arg.starts_with('%') && arg != &"%%"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace("%%", "%");
    Some(App {
        name: get.get("Name")?.clone(),
        exec,
        terminal: yes("Terminal"),
        icon: get.get("Icon").cloned(),
    })
}

fn main() {
    let others: Vec<u32> = pids("agm-menu").into_iter().filter(|p| *p != std::process::id()).collect();
    if !others.is_empty() {
        for pid in others {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        return;
    }

    let mut hidden = HashSet::new();
    let mut names = HashMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--hide", Some(id)) => {
                hidden.insert(id);
            }
            ("--name", Some(rename)) => {
                if let Some((id, name)) = rename.split_once('=') {
                    names.insert(id.to_string(), name.to_string());
                }
            }
            _ => {}
        }
    }

    let conn = Connection::connect_to_env().expect("no Wayland display");
    let (globals, queue) = registry_queue_init(&conn).unwrap();
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");

    let layer = layer_shell.create_layer_surface(&qh, compositor.create_surface(&qh), Layer::Top, Some("agm-menu"), None);
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.commit();

    let mut event_loop: EventLoop<Menu> = EventLoop::try_new().unwrap();
    let mut menu = Menu {
        handle: event_loop.handle(),
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        pool: SlotPool::new(240 * 320 * 4, &shm).expect("shm pool"),
        shm,
        layer,
        size: (0, 0),
        keyboard: None,
        touch: None,
        touched: None,
        shell: Shell::load(),
        apps: load_apps(&hidden, &names),
        selected: 0,
        scroll: 0,
        mode: current_mode(),
    };

    let handle = event_loop.handle();
    WaylandSource::new(conn, queue).insert(handle.clone()).unwrap();
    handle
        .insert_source(Signals::new(&[Signal::SIGTERM, Signal::SIGINT]).unwrap(), |_, _, menu| menu.close())
        .unwrap();

    loop {
        event_loop.dispatch(None, &mut menu).unwrap();
    }
}

impl LayerShellHandler for Menu {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.close();
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        self.size = configure.new_size;
        self.draw();
    }
}

impl KeyboardHandler for Menu {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        // t9 may switch the mode back when the focused text field loses focus.
        self.set_mode("menu");
        self.handle
            .insert_source(Timer::from_duration(Duration::from_millis(200)), |_, _, menu| {
                menu.set_mode("menu");
                TimeoutAction::Drop
            })
            .unwrap();
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(event.raw_code);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        if matches!(event.raw_code, KEY_UP | KEY_DOWN | KEY_LEFT | KEY_RIGHT) {
            self.key(event.raw_code);
        }
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: Modifiers, _: RawModifiers, _: u32) {}
}

impl TouchHandler for Menu {
    fn down(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, _: u32, _: wl_surface::WlSurface, id: i32, (x, y): (f64, f64)) {
        let (x, y) = (x as f32, y as f32);
        let hit = (0..COLS * ROWS).find(|&k| {
            let c = self.cell(k);
            x >= c.left() && x < c.right() && y >= c.top() && y < c.bottom()
        });
        if let Some(index) = hit.map(|k| self.scroll * COLS + k).filter(|&i| i < self.apps.len()) {
            self.touched = Some((id, index));
            self.select(index);
        } else if y > self.size.1 as f32 - SOFT_KEY_BAR {
            if x < self.size.0 as f32 / 2.0 {
                self.touched = Some((id, self.selected));
            } else {
                self.close();
            }
        }
    }
    fn up(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, _: u32, id: i32) {
        if let Some((touch, index)) = self.touched.take() {
            if touch == id {
                self.launch(index);
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

impl SeatHandler for Menu {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
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

impl CompositorHandler for Menu {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Menu {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Menu {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Menu {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_registry!(Menu);
smithay_client_toolkit::delegate_dispatch2!(Menu);
