use smithay_client_toolkit::shm::slot::SlotPool;
use tiny_skia::Pixmap;
use smithay_client_toolkit::reexports::client::protocol::{wl_shm, wl_surface::WlSurface};

pub fn present(pool: &mut SlotPool, surface: &WlSurface, pm: &Pixmap) {
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let (buffer, canvas) = pool
        .create_buffer(w, h, w * 4, wl_shm::Format::Xrgb8888)
        .expect("shm buffer");
    for (dst, src) in canvas.chunks_exact_mut(4).zip(pm.pixels()) {
        dst.copy_from_slice(&[src.blue(), src.green(), src.red(), 0xff]);
    }
    buffer.attach_to(surface).expect("attach");
    surface.damage_buffer(0, 0, w, h);
    surface.commit();
}
