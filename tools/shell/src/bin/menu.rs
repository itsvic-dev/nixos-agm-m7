use std::collections::HashSet;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use agm_shell::{Face, SOFT_KEY_BAR, SoftKeys, fill, pids, rounded_rect, wl::present};
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
use tiny_skia::{Color, FilterQuality, Paint, Pixmap, PixmapPaint, Rect, Shader, Transform};

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
const HEADER: f32 = 26.0;
const ICON: f32 = 40.0;

// Catppuccin Mocha.
const TEXT: [u8; 3] = [0xcd, 0xd6, 0xf4];
const OVERLAY: [u8; 3] = [0x93, 0x99, 0xb2];
const LAVENDER: [u8; 3] = [0xb4, 0xbe, 0xfe];
const SURFACE: [u8; 3] = [0x45, 0x47, 0x5a];
const CRUST: [u8; 3] = [0x11, 0x11, 0x1b];
const ACCENTS: [[u8; 3]; 5] = [
    [0xcb, 0xa6, 0xf7],
    [0x89, 0xb4, 0xfa],
    [0xa6, 0xe3, 0xa1],
    [0xfa, 0xb3, 0x87],
    [0x94, 0xe2, 0xd5],
];

struct App {
    name: String,
    exec: String,
    terminal: bool,
    icon: Option<Pixmap>,
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
    face: Face,
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
        let face = &self.face;
        face.background(&mut pm);
        let rgba = |c: [u8; 3], a: u8| Color::from_rgba8(c[0], c[1], c[2], a);
        fill(&mut pm, Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap(), rgba(CRUST, 120));
        fill(&mut pm, Rect::from_xywh(0.0, 0.0, w as f32, HEADER).unwrap(), rgba(CRUST, 140));

        let title = self.apps.get(self.selected).map_or("No apps", |a| &a.name);
        let title = face.fit(14.0, title, w as f32 - 16.0);
        face.label(&mut pm, 14.0, w as f32 / 2.0, 18.0, TEXT, &title);

        for k in 0..COLS * ROWS {
            let Some(app) = self.apps.get(self.scroll * COLS + k) else { break };
            let cell = self.cell(k);
            let cx = cell.left() + cell.width() / 2.0;

            if self.scroll * COLS + k == self.selected {
                let outer = Rect::from_xywh(cell.left() + 3.0, cell.top() + 2.0, cell.width() - 6.0, cell.height() - 4.0).unwrap();
                let inner = Rect::from_xywh(outer.left() + 1.5, outer.top() + 1.5, outer.width() - 3.0, outer.height() - 3.0).unwrap();
                rounded_rect(&mut pm, outer, 11.0, &solid(rgba(LAVENDER, 255)));
                rounded_rect(&mut pm, inner, 9.5, &solid(rgba(SURFACE, 230)));
            }

            let top = (cell.top() + (cell.height() - ICON - 16.0) / 2.0).round();
            let left = (cx - ICON / 2.0).round();
            match &app.icon {
                Some(icon) => {
                    let scale = ICON / icon.width().max(icon.height()) as f32;
                    let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
                    let t = Transform::from_row(scale, 0.0, 0.0, scale, left, top);
                    pm.draw_pixmap(0, 0, icon.as_ref(), &paint, t, None);
                }
                None => {
                    let accent = ACCENTS[app.name.bytes().map(usize::from).sum::<usize>() % ACCENTS.len()];
                    rounded_rect(&mut pm, Rect::from_xywh(left, top, ICON, ICON).unwrap(), 10.0, &solid(rgba(accent, 255)));
                    let letter: String = app.name.chars().take(1).collect::<String>().to_uppercase();
                    face.label(&mut pm, 22.0, cx, top + 28.0, CRUST, &letter);
                }
            }

            let name = face.fit(11.0, &app.name, cell.width() - 8.0);
            face.label(&mut pm, 11.0, cx, top + ICON + 13.0, TEXT, &name);
            face.label(&mut pm, 9.0, cell.left() + 10.0, cell.top() + 13.0, OVERLAY, &(k + 1).to_string());
        }

        face.soft_keys(&mut pm, &SoftKeys { left: "Open", right: "Back" });
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

fn solid(color: Color) -> Paint<'static> {
    Paint { shader: Shader::SolidColor(color), anti_alias: true, ..Default::default() }
}

fn current_mode() -> String {
    let out = Command::new(env!("AGM_SWAYMSG")).args(["-t", "get_binding_state"]).output();
    let json = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let mode = json.split('"').skip_while(|s| *s != "name").nth(2).unwrap_or("default");
    if mode == "menu" { "default".into() } else { mode.into() }
}

fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let data_home = std::env::var("XDG_DATA_HOME").unwrap_or(format!("{home}/.local/share"));
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or("/run/current-system/sw/share".into());
    std::iter::once(data_home.as_str()).chain(dirs.split(':')).filter(|d| !d.is_empty()).map(PathBuf::from).collect()
}

fn load_apps(hidden: &HashSet<String>) -> Vec<App> {
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
            if let Some(app) = parse_desktop(&path, &dirs) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn parse_desktop(path: &Path, dirs: &[PathBuf]) -> Option<App> {
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
        icon: get.get("Icon").and_then(|name| find_icon(name, dirs)),
    })
}

fn find_icon(name: &str, dirs: &[PathBuf]) -> Option<Pixmap> {
    if name.starts_with('/') {
        return Pixmap::load_png(name).ok();
    }
    let sizes = ["64x64", "48x48", "96x96", "128x128", "256x256", "32x32"];
    dirs.iter()
        .flat_map(|d| {
            sizes
                .iter()
                .map(move |s| d.join(format!("icons/hicolor/{s}/apps/{name}.png")))
                .chain(std::iter::once(d.join(format!("pixmaps/{name}.png"))))
        })
        .find_map(|p| Pixmap::load_png(p).ok())
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--hide" {
            hidden.extend(args.next());
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
        face: Face::load(),
        apps: load_apps(&hidden),
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
