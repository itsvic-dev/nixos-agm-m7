pub mod config;
pub mod status;
pub mod wl;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use chrono::{DateTime, Local};
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap,
    PixmapPaint, Point, PremultipliedColorU8, Rect, Shader, SpreadMode, Transform,
};

use status::Status;

const WHITE: [u8; 3] = [0xff, 0xff, 0xff];
const DIM: [u8; 3] = [0x9a, 0xa0, 0xa8];
const RED: [u8; 3] = [0xf3, 0x8b, 0xa8];
const GREEN: [u8; 3] = [0xa6, 0xe3, 0xa1];

pub const SOFT_KEY_BAR: f32 = 22.0;

pub struct SoftKeys<'a> {
    pub left: &'a str,
    pub right: &'a str,
}

pub struct Face {
    clock: FontArc,
    text: FontArc,
    wallpaper: Option<Pixmap>,
}

impl Face {
    pub fn load() -> Face {
        let font = |path: &str| {
            FontArc::try_from_vec(std::fs::read(path).expect(path)).expect(path)
        };
        Face {
            clock: font(env!("AGM_CLOCK_FONT")),
            text: font(env!("AGM_TEXT_FONT")),
            wallpaper: Pixmap::load_png(env!("AGM_WALLPAPER")).ok(),
        }
    }

    pub fn draw(
        &self,
        pm: &mut Pixmap,
        now: DateTime<Local>,
        status: &Status,
        keys: &SoftKeys,
        hint: Option<&str>,
    ) {
        let h = pm.height() as f32;
        self.background(pm);

        let clock_y = (h * 0.30).round();
        let h24 = config::Config::load().get_bool(config::CLOCK_24H, true);
        let (time, date) = if h24 { ("%H:%M", "%A, %-d %B") } else { ("%-I:%M", "%A, %-d %B, %p") };
        self.centered(pm, &self.clock, 68.0, clock_y, WHITE, &now.format(time).to_string());
        self.centered(pm, &self.text, 14.0, clock_y + 24.0, WHITE, &now.format(date).to_string());
        self.status_row(pm, clock_y + 46.0, status);

        if let Some(hint) = hint {
            self.centered(pm, &self.text, 13.0, h - SOFT_KEY_BAR - 14.0, WHITE, hint);
        }
        self.soft_keys(pm, keys);
    }

    pub fn soft_keys(&self, pm: &mut Pixmap, keys: &SoftKeys) {
        let (w, h) = (pm.width() as f32, pm.height() as f32);
        fill(pm, Rect::from_xywh(0.0, h - SOFT_KEY_BAR, w, SOFT_KEY_BAR).unwrap(), Color::from_rgba8(0, 0, 0, 150));
        let base = h - 7.0;
        self.text(pm, &self.text, 13.0, 7.0, base, WHITE, keys.left);
        let right = text_width(&self.text, 13.0, keys.right);
        self.text(pm, &self.text, 13.0, w - 7.0 - right, base, WHITE, keys.right);
    }

    /// Draws `s` with the text font and a shadow, centered on `cx`.
    pub fn label(&self, pm: &mut Pixmap, px: f32, cx: f32, base: f32, color: [u8; 3], s: &str) {
        let x = (cx - text_width(&self.text, px, s) / 2.0).round();
        self.text(pm, &self.text, px, x, base, color, s);
    }

    /// Draws `s` with the text font and a shadow, starting at `x`.
    pub fn label_left(&self, pm: &mut Pixmap, px: f32, x: f32, base: f32, color: [u8; 3], s: &str) {
        self.text(pm, &self.text, px, x, base, color, s);
    }

    pub fn width(&self, px: f32, s: &str) -> f32 {
        text_width(&self.text, px, s)
    }

    pub fn fit(&self, px: f32, s: &str, max: f32) -> String {
        ellipsize(&self.text, px, s, max)
    }

    pub fn background(&self, pm: &mut Pixmap) {
        let (w, h) = (pm.width() as f32, pm.height() as f32);
        pm.fill(Color::from_rgba8(0x1e, 0x1e, 0x2e, 0xff));
        if let Some(wp) = &self.wallpaper {
            let scale = (w / wp.width() as f32).max(h / wp.height() as f32);
            let dx = (w - wp.width() as f32 * scale) / 2.0;
            let dy = (h - wp.height() as f32 * scale) / 2.0;
            let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
            pm.draw_pixmap(0, 0, wp.as_ref(), &paint, Transform::from_row(scale, 0.0, 0.0, scale, dx, dy), None);
        }
        let shade = LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(0.0, h),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(0, 0, 0, 60)),
                GradientStop::new(0.55, Color::from_rgba8(0, 0, 0, 0)),
                GradientStop::new(1.0, Color::from_rgba8(0, 0, 0, 50)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap();
        let paint = Paint { shader: shade, ..Default::default() };
        pm.fill_rect(Rect::from_xywh(0.0, 0.0, w, h).unwrap(), &paint, Transform::identity(), None);
    }

    fn status_row(&self, pm: &mut Pixmap, base: f32, status: &Status) {
        const PX: f32 = 13.0;
        const GAP: f32 = 4.0;
        let mut items: Vec<(f32, Box<dyn Fn(&mut Pixmap, f32)>)> = Vec::new();

        if let Some(bat) = &status.battery {
            let label = format!("{}%", bat.percent);
            let tw = text_width(&self.text, PX, &label);
            let color = if bat.charging {
                GREEN
            } else if bat.percent <= 15 {
                RED
            } else {
                WHITE
            };
            let face = self;
            items.push((
                20.0 + GAP + tw,
                Box::new(move |pm, x| {
                    battery_icon(pm, x, base - 9.0, bat.percent, color);
                    face.text(pm, &face.text, PX, x + 20.0 + GAP, base, WHITE, &label);
                }),
            ));
        }

        let (bars, name) = match &status.wifi {
            Some(wifi) => (wifi.bars, ellipsize(&self.text, PX, &wifi.name, 110.0)),
            None => (0, "No Wi-Fi".to_string()),
        };
        let tw = text_width(&self.text, PX, &name);
        let face = self;
        items.push((
            15.0 + GAP + tw,
            Box::new(move |pm, x| {
                wifi_icon(pm, x, base, bars);
                let color = if bars > 0 { WHITE } else { DIM };
                face.text(pm, &face.text, PX, x + 15.0 + GAP, base, color, &name);
            }),
        ));

        let sep = 16.0;
        let total: f32 = items.iter().map(|(w, _)| w).sum::<f32>() + sep * (items.len() - 1) as f32;
        let mut x = ((pm.width() as f32 - total) / 2.0).round();
        for (w, draw) in items {
            draw(pm, x);
            x += w + sep;
        }
    }

    fn centered(&self, pm: &mut Pixmap, font: &FontArc, px: f32, base: f32, color: [u8; 3], s: &str) {
        let x = ((pm.width() as f32 - text_width(font, px, s)) / 2.0).round();
        self.text(pm, font, px, x, base, color, s);
    }

    fn text(&self, pm: &mut Pixmap, font: &FontArc, px: f32, x: f32, base: f32, color: [u8; 3], s: &str) {
        draw_text(pm, font, px, x + 1.0, base + 1.0, [0, 0, 0], 0.6, s);
        draw_text(pm, font, px, x, base, color, 1.0, s);
    }
}

pub fn text_width(font: &FontArc, px: f32, s: &str) -> f32 {
    let f = font.as_scaled(PxScale::from(px));
    let mut width = 0.0;
    let mut prev = None;
    for c in s.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            width += f.kern(p, id);
        }
        width += f.h_advance(id);
        prev = Some(id);
    }
    width
}

fn ellipsize(font: &FontArc, px: f32, s: &str, max: f32) -> String {
    if text_width(font, px, s) <= max {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && text_width(font, px, &format!("{out}…")) > max {
        out.pop();
    }
    format!("{out}…")
}

fn draw_text(pm: &mut Pixmap, font: &FontArc, px: f32, x: f32, base: f32, color: [u8; 3], alpha: f32, s: &str) {
    let scale = PxScale::from(px);
    let f = font.as_scaled(scale);
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let mut caret = x;
    let mut prev = None;
    for c in s.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            caret += f.kern(p, id);
        }
        let glyph = id.with_scale_and_position(scale, point(caret, base));
        caret += f.h_advance(id);
        prev = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let bounds = outline.px_bounds();
        let pixels = pm.pixels_mut();
        outline.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i32 + gx as i32;
            let py = bounds.min.y as i32 + gy as i32;
            if px >= 0 && py >= 0 && px < w && py < h {
                let i = (py * w + px) as usize;
                pixels[i] = blend(pixels[i], color, coverage.min(1.0) * alpha);
            }
        });
    }
}

fn blend(dst: PremultipliedColorU8, src: [u8; 3], a: f32) -> PremultipliedColorU8 {
    let mix = |s: u8, d: u8| (s as f32 * a + d as f32 * (1.0 - a)).round() as u8;
    let alpha = mix(255, dst.alpha());
    PremultipliedColorU8::from_rgba(
        mix(src[0], dst.red()).min(alpha),
        mix(src[1], dst.green()).min(alpha),
        mix(src[2], dst.blue()).min(alpha),
        alpha,
    )
    .unwrap()
}

pub fn fill(pm: &mut Pixmap, rect: Rect, color: Color) {
    let paint = Paint { shader: Shader::SolidColor(color), ..Default::default() };
    pm.fill_rect(rect, &paint, Transform::identity(), None);
}

pub fn rounded_rect(pm: &mut Pixmap, rect: Rect, radius: f32, paint: &Paint) {
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(r - radius, t);
    pb.quad_to(r, t, r, t + radius);
    pb.line_to(r, b - radius);
    pb.quad_to(r, b, r - radius, b);
    pb.line_to(l + radius, b);
    pb.quad_to(l, b, l, b - radius);
    pb.line_to(l, t + radius);
    pb.quad_to(l, t, l + radius, t);
    pb.close();
    pm.fill_path(&pb.finish().unwrap(), paint, FillRule::Winding, Transform::identity(), None);
}

/// Process IDs whose `comm` is `name`, without zombies.
pub fn pids(name: &str) -> Vec<u32> {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|p| {
            let pid = p.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(p.path().join("stat")).ok()?;
            let (comm, rest) = stat.split_once(") ")?;
            (comm.ends_with(&format!("({name}")) && !rest.starts_with('Z')).then_some(pid)
        })
        .collect()
}

fn rgb(c: [u8; 3], a: u8) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], a)
}

fn battery_icon(pm: &mut Pixmap, x: f32, y: f32, percent: u8, color: [u8; 3]) {
    let mut pb = PathBuilder::new();
    pb.push_rect(Rect::from_xywh(x, y, 18.0, 10.0).unwrap());
    pb.push_rect(Rect::from_xywh(x + 1.0, y + 1.0, 16.0, 8.0).unwrap());
    let paint = Paint { shader: Shader::SolidColor(rgb(WHITE, 255)), ..Default::default() };
    pm.fill_path(&pb.finish().unwrap(), &paint, FillRule::EvenOdd, Transform::identity(), None);
    fill(pm, Rect::from_xywh(x + 18.0, y + 3.0, 2.0, 4.0).unwrap(), rgb(WHITE, 255));
    let level = (14.0 * percent.min(100) as f32 / 100.0).round().max(1.0);
    fill(pm, Rect::from_xywh(x + 2.0, y + 2.0, level, 6.0).unwrap(), rgb(color, 255));
}

fn wifi_icon(pm: &mut Pixmap, x: f32, base: f32, bars: u8) {
    for i in 0..4u8 {
        let bh = 3.0 + 2.5 * i as f32;
        let color = if i < bars { rgb(WHITE, 255) } else { rgb(WHITE, 70) };
        fill(pm, Rect::from_xywh(x + 4.0 * i as f32, base - bh, 3.0, bh).unwrap(), color);
    }
}
