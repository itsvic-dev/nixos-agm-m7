use std::time::Duration;

use agm_shell::{shell::{BAR, Shell}, status::Status, wl::present};
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

struct Bar {
    registry: RegistryState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    size: (u32, u32),
    shell: Shell,
    bus: Option<zbus::blocking::Connection>,
    status: Status,
}

impl Bar {
    fn draw(&mut self) {
        let (w, h) = self.size;
        let Some(mut pm) = Pixmap::new(w, h) else { return };
        self.shell.status_bar(&mut pm, &self.status, Local::now());
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
        Layer::Top,
        Some("agm-bar"),
        None,
    );
    layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
    layer.set_size(0, BAR as u32);
    layer.set_exclusive_zone(BAR as i32);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();

    let bus = zbus::blocking::Connection::system().ok();
    let mut home = Bar {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        pool: SlotPool::new(240 * 320 * 4, &shm).expect("shm pool"),
        shm,
        layer,
        size: (0, 0),
        shell: Shell::load(),
        status: Status::read(bus.as_ref()),
        bus,
    };

    let mut event_loop: EventLoop<Bar> = EventLoop::try_new().unwrap();
    let handle = event_loop.handle();
    WaylandSource::new(conn, queue).insert(handle.clone()).unwrap();

    handle
        .insert_source(Timer::from_duration(until_next_tick()), |_, _, home| {
            home.refresh();
            TimeoutAction::ToDuration(until_next_tick())
        })
        .unwrap();

    let (changed, rx) = channel::channel();
    agm_shell::netwatch::watch(move || changed.send(()).is_ok());
    handle
        .insert_source(rx, |event, _, bar| {
            if let Event::Msg(()) = event {
                bar.refresh();
            }
        })
        .unwrap();

    loop {
        event_loop.dispatch(None, &mut home).unwrap();
    }
}

fn until_next_tick() -> Duration {
    let now = Local::now();
    let into = now.second() % 10;
    Duration::from_secs((10 - into) as u64) - Duration::from_nanos(now.nanosecond() as u64 % 1_000_000_000)
}

impl LayerShellHandler for Bar {
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

impl CompositorHandler for Bar {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Bar {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Bar {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Bar {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_registry!(Bar);
smithay_client_toolkit::delegate_dispatch2!(Bar);
