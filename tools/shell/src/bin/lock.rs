use std::time::{Duration, Instant};

use agm_shell::{Face, SoftKeys, status::Status, wl::present};
use chrono::Local;
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
            protocol::{wl_keyboard, wl_output, wl_seat, wl_surface},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use tiny_skia::Pixmap;

const KEY_MENU: u32 = 139;
const KEY_NUMERIC_STAR: u32 = 522;
const COMBO: Duration = Duration::from_millis(1500);

struct Lock {
    conn: Connection,
    handle: LoopHandle<'static, Lock>,
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    shm: Shm,
    pool: SlotPool,
    lock: Option<SessionLock>,
    surfaces: Vec<(SessionLockSurface, (u32, u32))>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    face: Face,
    bus: Option<zbus::blocking::Connection>,
    status: Status,
    menu_at: Option<Instant>,
    done: bool,
}

impl Lock {
    fn draw(&mut self) {
        let combo = self.menu_at.is_some_and(|t| t.elapsed() < COMBO);
        let hint = if combo { "Now press *" } else { "Press Menu, then * to unlock" };
        let keys = SoftKeys { left: "Unlock", right: "" };
        let now = Local::now();
        for (surface, (w, h)) in &self.surfaces {
            let Some(mut pm) = Pixmap::new(*w, *h) else { continue };
            self.face.draw(&mut pm, now, &self.status, &keys, Some(hint));
            present(&mut self.pool, surface.wl_surface(), &pm);
        }
    }

    fn key(&mut self, code: u32) {
        match code {
            KEY_MENU => {
                self.menu_at = Some(Instant::now());
                self.handle
                    .insert_source(Timer::from_duration(COMBO), |_, _, lock| {
                        lock.draw();
                        TimeoutAction::Drop
                    })
                    .unwrap();
            }
            KEY_NUMERIC_STAR if self.menu_at.take().is_some_and(|t| t.elapsed() < COMBO) => {
                if let Some(lock) = self.lock.take() {
                    lock.unlock();
                    let _ = self.conn.roundtrip();
                }
                self.done = true;
                return;
            }
            _ => self.menu_at = None,
        }
        self.draw();
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("no Wayland display");
    let (globals, mut queue) = registry_queue_init(&conn).unwrap();
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let lock_state = SessionLockState::new(&globals, &qh);

    let mut event_loop: EventLoop<Lock> = EventLoop::try_new().unwrap();
    let bus = zbus::blocking::Connection::system().ok();
    let mut app = Lock {
        conn: conn.clone(),
        handle: event_loop.handle(),
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        pool: SlotPool::new(240 * 320 * 4, &shm).expect("shm pool"),
        shm,
        lock: None,
        surfaces: Vec::new(),
        keyboard: None,
        face: Face::load(),
        status: Status::read(bus.as_ref()),
        bus,
        menu_at: None,
        done: false,
    };
    queue.roundtrip(&mut app).unwrap();

    let lock = lock_state.lock(&qh).expect("ext-session-lock-v1");
    for output in app.outputs.outputs() {
        let surface = lock.create_lock_surface(compositor.create_surface(&qh), &output, &qh);
        app.surfaces.push((surface, (0, 0)));
    }
    app.lock = Some(lock);

    WaylandSource::new(conn, queue).insert(event_loop.handle()).unwrap();
    event_loop
        .handle()
        .insert_source(Timer::from_duration(Duration::from_secs(30)), |_, _, lock| {
            lock.status = Status::read(lock.bus.as_ref());
            lock.draw();
            TimeoutAction::ToDuration(Duration::from_secs(30))
        })
        .unwrap();

    while !app.done {
        event_loop.dispatch(None, &mut app).unwrap();
    }
}

impl SessionLockHandler for Lock {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {}

    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        eprintln!("agm-lock: the session is already locked or cannot be locked");
        std::process::exit(1);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _: u32,
    ) {
        for (s, size) in &mut self.surfaces {
            if s.wl_surface() == surface.wl_surface() {
                *size = configure.new_size;
            }
        }
        self.draw();
    }
}

impl KeyboardHandler for Lock {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(event.raw_code);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: Modifiers, _: RawModifiers, _: u32) {}
}

impl SeatHandler for Lock {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl CompositorHandler for Lock {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Lock {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Lock {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Lock {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_registry!(Lock);
smithay_client_toolkit::delegate_dispatch2!(Lock);
