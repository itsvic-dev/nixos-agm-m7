use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Shader, Transform};

use crate::theme::Rgb;

pub fn rgba(c: Rgb, a: u8) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], a)
}

pub fn paint(c: Rgb, a: u8) -> Paint<'static> {
    Paint { shader: Shader::SolidColor(rgba(c, a)), anti_alias: true, ..Default::default() }
}

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)).unwrap()
}

pub fn fill(pm: &mut Pixmap, r: Rect, c: Rgb, a: u8) {
    pm.fill_rect(r, &paint(c, a), Transform::identity(), None);
}

pub fn rounded(pm: &mut Pixmap, r: Rect, radius: f32, c: Rgb, a: u8) {
    let radius = radius.min(r.width() / 2.0).min(r.height() / 2.0);
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    // 0.5523 places cubic control points on a circle quadrant.
    let k = radius * 0.5523;
    let mut pb = PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(rr - radius, t);
    pb.cubic_to(rr - radius + k, t, rr, t + radius - k, rr, t + radius);
    pb.line_to(rr, b - radius);
    pb.cubic_to(rr, b - radius + k, rr - radius + k, b, rr - radius, b);
    pb.line_to(l + radius, b);
    pb.cubic_to(l + radius - k, b, l, b - radius + k, l, b - radius);
    pb.line_to(l, t + radius);
    pb.cubic_to(l, t + radius - k, l + radius - k, t, l + radius, t);
    pb.close();
    pm.fill_path(&pb.finish().unwrap(), &paint(c, a), FillRule::Winding, Transform::identity(), None);
}

/// A ring of `width` around `r`, for selection outlines.
pub fn ring(pm: &mut Pixmap, r: Rect, radius: f32, width: f32, c: Rgb, inner: Rgb) {
    rounded(pm, r, radius, c, 255);
    let i = rect(r.left() + width, r.top() + width, r.width() - 2.0 * width, r.height() - 2.0 * width);
    rounded(pm, i, (radius - width).max(0.0), inner, 255);
}

pub fn image(pm: &mut Pixmap, img: &Pixmap, x: f32, y: f32) {
    pm.draw_pixmap(x.round() as i32, y.round() as i32, img.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}
