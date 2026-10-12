use std::io::Read;
use std::process::Command;
use std::time::{Duration, Instant};

use agm_shell::{Face, SoftKeys, pids, status::Status, wl::present};
use chrono::{Local, Timelike};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            EventLoop,
            channel::{self, Event},
            timer::{TimeoutAction, Timer},
        },
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, QueueHandle,
            globals::registry_queue_init,
            protocol::{wl_output, wl_surface},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use tiny_skia::Pixmap;

const KEY_MENU: u16 = 139;
const KEY_NUMERIC_STAR: u16 = 522;
const COMBO: Duration = Duration::from_millis(1500);

struct Home {
    registry: RegistryState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    size: (u32, u32),
    face: Face,
    bus: Option<zbus::blocking::Connection>,
    status: Status,
}

impl Home {
    fn draw(&mut self) {
        let (w, h) = self.size;
        let Some(mut pm) = Pixmap::new(w, h) else { return };
        let keys = SoftKeys { left: "Menu", right: "" };
        self.face.draw(&mut pm, Local::now(), &self.status, &keys, None);
        present(&mut self.pool, self.layer.wl_surface(), &pm);
    }

    fn refresh(&mut self) {
        self.status = Status::read(self.bus.as_ref());
        self.draw();
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("no Wayland display");
    let (globals, queue) = registry_queue_init(&conn).unwrap();
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");

    let layer = layer_shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Background,
        Some("agm-home"),
        None,
    );
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();

    let bus = zbus::blocking::Connection::system().ok();
    let mut home = Home {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        pool: SlotPool::new(240 * 320 * 4, &shm).expect("shm pool"),
        shm,
        layer,
        size: (0, 0),
        face: Face::load(),
        status: Status::read(bus.as_ref()),
        bus,
    };

    let mut event_loop: EventLoop<Home> = EventLoop::try_new().unwrap();
    let handle = event_loop.handle();
    WaylandSource::new(conn, queue).insert(handle.clone()).unwrap();

    handle
        .insert_source(Timer::from_duration(until_next_tick()), |_, _, home| {
            home.refresh();
            TimeoutAction::ToDuration(until_next_tick())
        })
        .unwrap();

    let (keys, rx) = channel::channel();
    watch_keys(keys);
    let mut menu_at: Option<Instant> = None;
    handle
        .insert_source(rx, move |event, _, _| {
            let Event::Msg(code) = event else { return };
            match code {
                KEY_MENU if !locked() => menu_at = Some(Instant::now()),
                KEY_NUMERIC_STAR if menu_at.take().is_some_and(|t| t.elapsed() < COMBO) => lock(),
                _ => menu_at = None,
            }
        })
        .unwrap();

    loop {
        event_loop.dispatch(None, &mut home).unwrap();
    }
}

fn until_next_tick() -> Duration {
    let now = Local::now();
    let into = now.second() % 30;
    Duration::from_secs((30 - into) as u64) - Duration::from_nanos(now.nanosecond() as u64 % 1_000_000_000)
}

fn watch_keys(tx: channel::Sender<u16>) {
    let Ok(dir) = std::fs::read_dir("/dev/input") else { return };
    for entry in dir.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }
        let Ok(mut dev) = std::fs::File::open(entry.path()) else { continue };
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut ev = [0u8; 24];
            while dev.read_exact(&mut ev).is_ok() {
                let kind = u16::from_ne_bytes([ev[16], ev[17]]);
                let code = u16::from_ne_bytes([ev[18], ev[19]]);
                let value = i32::from_ne_bytes([ev[20], ev[21], ev[22], ev[23]]);
                if kind == 1 && value == 1 && tx.send(code).is_err() {
                    break;
                }
            }
        });
    }
}

fn locked() -> bool {
    !pids("agm-lock").is_empty()
}

fn lock() {
    for pid in pids("agm-menu") {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    let exe = std::env::current_exe().unwrap().with_file_name("agm-lock");
    if let Ok(mut child) = Command::new(exe).spawn() {
        std::thread::spawn(move || child.wait());
    }
}

impl LayerShellHandler for Home {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        std::process::exit(0);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.size = configure.new_size;
        self.draw();
    }
}

impl CompositorHandler for Home {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Home {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Home {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Home {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_registry!(Home);
smithay_client_toolkit::delegate_dispatch2!(Home);
