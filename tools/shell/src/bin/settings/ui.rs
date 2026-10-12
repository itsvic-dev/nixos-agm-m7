use agm_shell::{Face, SOFT_KEY_BAR, SoftKeys, fill, rounded_rect};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Rect, Shader, Stroke, Transform};

pub const HEADER: f32 = 24.0;
pub const ROW: f32 = 34.0;

// Catppuccin Mocha.
pub const TEXT: [u8; 3] = [0xcd, 0xd6, 0xf4];
pub const SUBTEXT: [u8; 3] = [0xa6, 0xad, 0xc8];
pub const LAVENDER: [u8; 3] = [0xb4, 0xbe, 0xfe];
pub const GREEN: [u8; 3] = [0xa6, 0xe3, 0xa1];
pub const BASE: [u8; 3] = [0x1e, 0x1e, 0x2e];
pub const MANTLE: [u8; 3] = [0x18, 0x18, 0x25];
pub const SURFACE0: [u8; 3] = [0x31, 0x32, 0x44];
pub const SURFACE1: [u8; 3] = [0x45, 0x47, 0x5a];
pub const OVERLAY: [u8; 3] = [0x6c, 0x70, 0x86];

pub enum Kind {
    Info,
    Link,
    Button,
    Toggle(bool),
    Slider(f32),
    Bar(f32),
    Network { bars: u8, locked: bool },
    Field { text: String, preedit: String, focused: bool },
}

pub struct Row {
    pub title: String,
    pub value: String,
    pub kind: Kind,
}

impl Row {
    pub fn new(title: impl Into<String>, value: impl Into<String>, kind: Kind) -> Row {
        Row { title: title.into(), value: value.into(), kind }
    }
}

pub fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], 255)
}

fn solid(c: [u8; 3]) -> Paint<'static> {
    Paint { shader: Shader::SolidColor(rgb(c)), anti_alias: true, ..Default::default() }
}

pub fn visible_rows(height: u32) -> usize {
    ((height as f32 - HEADER - SOFT_KEY_BAR) / ROW).floor().max(1.0) as usize
}

pub fn row_at(height: u32, scroll: usize, y: f32) -> Option<usize> {
    if y < HEADER || y > height as f32 - SOFT_KEY_BAR {
        return None;
    }
    let i = ((y - HEADER) / ROW) as usize;
    (i < visible_rows(height)).then_some(scroll + i)
}

pub struct View<'a> {
    pub title: &'a str,
    pub rows: &'a [Row],
    pub selected: usize,
    pub scroll: usize,
    pub message: Option<&'a str>,
    pub keys: SoftKeys<'a>,
}

pub fn draw(face: &Face, pm: &mut Pixmap, view: &View) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    pm.fill(rgb(BASE));
    fill(pm, Rect::from_xywh(0.0, 0.0, w, HEADER).unwrap(), rgb(MANTLE));
    face.label_left(pm, 14.0, 10.0, 17.0, TEXT, &face.fit(14.0, view.title, w - 20.0));

    for (i, row) in view.rows.iter().enumerate().skip(view.scroll).take(visible_rows(h as u32)) {
        let top = HEADER + (i - view.scroll) as f32 * ROW;
        if i == view.selected {
            let r = Rect::from_xywh(4.0, top + 2.0, w - 8.0, ROW - 4.0).unwrap();
            rounded_rect(pm, r, 8.0, &solid(SURFACE0));
            rounded_rect(pm, Rect::from_xywh(4.0, top + 8.0, 3.0, ROW - 16.0).unwrap(), 1.5, &solid(LAVENDER));
        }
        draw_row(face, pm, row, top, w);
    }

    if view.rows.len() > visible_rows(h as u32) {
        let track = h - HEADER - SOFT_KEY_BAR;
        let len = (track * visible_rows(h as u32) as f32 / view.rows.len() as f32).max(12.0);
        let pos = HEADER + (track - len) * view.scroll as f32 / (view.rows.len() - visible_rows(h as u32)) as f32;
        rounded_rect(pm, Rect::from_xywh(w - 3.0, pos, 2.0, len).unwrap(), 1.0, &solid(OVERLAY));
    }

    if let Some(message) = view.message {
        let r = Rect::from_xywh(8.0, h - SOFT_KEY_BAR - 30.0, w - 16.0, 24.0).unwrap();
        rounded_rect(pm, r, 8.0, &solid(SURFACE1));
        face.label(pm, 12.0, w / 2.0, h - SOFT_KEY_BAR - 14.0, TEXT, &face.fit(12.0, message, w - 28.0));
    }
    face.soft_keys(pm, &view.keys);
}

fn draw_row(face: &Face, pm: &mut Pixmap, row: &Row, top: f32, w: f32) {
    let x = 14.0;
    let right = w - 12.0;
    let two_lines = !row.value.is_empty() && !matches!(row.kind, Kind::Field { .. });
    let title_base = if two_lines { top + 15.0 } else { top + 21.0 };
    let value_base = top + 28.0;

    match &row.kind {
        Kind::Toggle(on) => {
            let r = Rect::from_xywh(right - 26.0, top + 10.0, 26.0, 14.0).unwrap();
            rounded_rect(pm, r, 7.0, &solid(if *on { LAVENDER } else { SURFACE1 }));
            let knob = if *on { right - 12.0 } else { right - 24.0 };
            rounded_rect(pm, Rect::from_xywh(knob, top + 12.0, 10.0, 10.0).unwrap(), 5.0, &solid(if *on { BASE } else { SUBTEXT }));
        }
        Kind::Slider(f) | Kind::Bar(f) => {
            let width = right - x;
            rounded_rect(pm, Rect::from_xywh(x, top + 24.0, width, 4.0).unwrap(), 2.0, &solid(SURFACE1));
            let filled = (width * f.clamp(0.0, 1.0)).max(4.0);
            rounded_rect(pm, Rect::from_xywh(x, top + 24.0, filled, 4.0).unwrap(), 2.0, &solid(LAVENDER));
            if matches!(row.kind, Kind::Slider(_)) {
                rounded_rect(pm, Rect::from_xywh(x + filled - 5.0, top + 21.0, 10.0, 10.0).unwrap(), 5.0, &solid(TEXT));
            }
            face.label_left(pm, 13.0, x, top + 15.0, TEXT, &row.title);
            let vw = face.width(11.0, &row.value);
            face.label_left(pm, 11.0, right - vw, top + 15.0, SUBTEXT, &row.value);
            return;
        }
        Kind::Network { bars, locked } => {
            for i in 0..4u8 {
                let bh = 3.0 + 2.5 * i as f32;
                let c = if i < *bars { TEXT } else { SURFACE1 };
                fill(pm, Rect::from_xywh(right - 15.0 + 4.0 * i as f32, top + 22.0 - bh, 3.0, bh).unwrap(), rgb(c));
            }
            if *locked {
                lock_icon(pm, right - 28.0, top + 12.0);
            }
        }
        Kind::Field { text, preedit, focused } => {
            let r = Rect::from_xywh(x - 4.0, top + 4.0, right - x + 8.0, ROW - 8.0).unwrap();
            rounded_rect(pm, r, 6.0, &solid(if *focused { LAVENDER } else { SURFACE1 }));
            let inner = Rect::from_xywh(r.left() + 1.5, r.top() + 1.5, r.width() - 3.0, r.height() - 3.0).unwrap();
            rounded_rect(pm, inner, 5.0, &solid(MANTLE));
            let shown = format!("{text}{preedit}");
            let shown = if shown.is_empty() && !*focused { row.title.clone() } else { shown };
            let mut tail = shown.clone();
            while face.width(13.0, &tail) > r.width() - 16.0 && !tail.is_empty() {
                tail.remove(0);
            }
            let color = if text.is_empty() && preedit.is_empty() && !*focused { OVERLAY } else { TEXT };
            face.label_left(pm, 13.0, x + 2.0, top + 22.0, color, &tail);
            if *focused {
                let cx = x + 3.0 + face.width(13.0, &tail);
                fill(pm, Rect::from_xywh(cx, top + 10.0, 1.5, 15.0).unwrap(), rgb(LAVENDER));
            }
            return;
        }
        Kind::Info | Kind::Link | Kind::Button => {}
    }

    let color = if matches!(row.kind, Kind::Button) { LAVENDER } else { TEXT };
    let title_max = right - x - if matches!(row.kind, Kind::Toggle(_) | Kind::Network { .. }) { 34.0 } else { 0.0 };
    face.label_left(pm, 13.0, x, title_base, color, &face.fit(13.0, &row.title, title_max));
    if two_lines {
        let value = face.fit(11.0, &row.value, title_max);
        let value_color = if row.value == "Connected" { GREEN } else { SUBTEXT };
        face.label_left(pm, 11.0, x, value_base, value_color, &value);
    }
    if matches!(row.kind, Kind::Link) {
        face.label_left(pm, 13.0, right - 6.0, top + 21.0, OVERLAY, "›");
    }
}

fn lock_icon(pm: &mut Pixmap, x: f32, y: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(x + 1.5, y + 5.0);
    pb.line_to(x + 1.5, y + 3.0);
    pb.quad_to(x + 1.5, y, x + 4.0, y);
    pb.quad_to(x + 6.5, y, x + 6.5, y + 3.0);
    pb.line_to(x + 6.5, y + 5.0);
    let stroke = Stroke { width: 1.3, ..Default::default() };
    pm.stroke_path(&pb.finish().unwrap(), &solid(SUBTEXT), &stroke, Transform::identity(), None);
    rounded_rect(pm, Rect::from_xywh(x, y + 5.0, 8.0, 6.0).unwrap(), 1.5, &solid(SUBTEXT));
}
