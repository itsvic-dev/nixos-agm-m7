use agm_shell::draw::{fill, image, rect, rounded};
use agm_shell::shell::{SOFT_KEY_BAR, Shell};
use agm_shell::text::Weight;
use agm_shell::theme::{LIGHT, WHITE};
use tiny_skia::Pixmap;

pub const HEADER: f32 = 34.0;
pub const ROW: f32 = 34.0;
const MARGIN: f32 = 8.0;

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
    pub icon: Option<String>,
}

impl Row {
    pub fn new(title: impl Into<String>, value: impl Into<String>, kind: Kind) -> Row {
        Row { title: title.into(), value: value.into(), kind, icon: None }
    }

    pub fn icon(mut self, name: impl Into<String>) -> Row {
        self.icon = Some(name.into());
        self
    }
}

pub fn visible_rows(height: u32) -> usize {
    ((height as f32 - HEADER - SOFT_KEY_BAR - MARGIN) / ROW).floor().max(1.0) as usize
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
    pub right: &'a str,
}

pub fn draw(shell: &mut Shell, pm: &mut Pixmap, view: &View) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    pm.fill(agm_shell::draw::rgba(LIGHT.bg, 255));
    let title = shell.fonts.fit(Weight::SemiBold, 18.0, view.title, w - 24.0);
    shell.fonts.draw(pm, Weight::SemiBold, 18.0, 12.0, 25.0, LIGHT.text, &title);

    let shown: Vec<_> = view.rows.iter().enumerate().skip(view.scroll).take(visible_rows(h as u32)).collect();
    if !shown.is_empty() {
        let card = rect(MARGIN, HEADER, w - 2.0 * MARGIN, shown.len() as f32 * ROW);
        rounded(pm, card, 12.0, LIGHT.surface, 255);
    }
    for (n, (i, row)) in shown.iter().enumerate() {
        let top = HEADER + n as f32 * ROW;
        if *i == view.selected {
            let r = rect(MARGIN, top, w - 2.0 * MARGIN, ROW);
            let radius = if shown.len() == 1 { 12.0 } else if n == 0 || n == shown.len() - 1 { 12.0 } else { 0.0 };
            rounded(pm, r, radius, LIGHT.accent_tint, 255);
            if radius > 0.0 && shown.len() > 1 {
                // Square off the corners that face the neighbouring rows.
                let flat = if n == 0 { rect(MARGIN, top + ROW / 2.0, w - 2.0 * MARGIN, ROW / 2.0) } else { rect(MARGIN, top, w - 2.0 * MARGIN, ROW / 2.0) };
                fill(pm, flat, LIGHT.accent_tint, 255);
            }
        }
        if n > 0 {
            let x = if row.icon.is_some() { MARGIN + 40.0 } else { MARGIN + 12.0 };
            fill(pm, rect(x, top, w - MARGIN - x, 1.0), LIGHT.separator, 255);
        }
        draw_row(shell, pm, row, top, w, *i == view.selected);
    }

    if view.rows.len() > visible_rows(h as u32) {
        let track = h - HEADER - SOFT_KEY_BAR - MARGIN;
        let len = (track * visible_rows(h as u32) as f32 / view.rows.len() as f32).max(12.0);
        let pos = HEADER + (track - len) * view.scroll as f32 / (view.rows.len() - visible_rows(h as u32)) as f32;
        rounded(pm, rect(w - 4.0, pos, 2.5, len), 1.25, LIGHT.subtext, 120);
    }

    if let Some(message) = view.message {
        let text = shell.fonts.fit(Weight::Medium, 11.0, message, w - 40.0);
        let tw = shell.fonts.width(Weight::Medium, 11.0, &text) + 24.0;
        rounded(pm, rect((w - tw) / 2.0, h - SOFT_KEY_BAR - 32.0, tw, 24.0), 12.0, LIGHT.text, 235);
        shell.fonts.draw_centered(pm, Weight::Medium, 11.0, w / 2.0, h - SOFT_KEY_BAR - 16.0, WHITE, &text);
    }
    shell.soft_keys(pm, &LIGHT, "", view.right);
}

fn draw_row(shell: &mut Shell, pm: &mut Pixmap, row: &Row, top: f32, w: f32, selected: bool) {
    let mut x = MARGIN + 12.0;
    let right = w - MARGIN - 10.0;

    if let Some(icon) = &row.icon {
        let bg = if selected { WHITE } else { LIGHT.accent_tint };
        rounded(pm, rect(MARGIN + 8.0, top + 5.0, 24.0, 24.0), 12.0, bg, 255);
        if let Some(img) = shell.icons.get(icon, 14, Some(LIGHT.accent)) {
            image(pm, img, MARGIN + 13.0, top + 10.0);
        }
        x = MARGIN + 40.0;
    }

    let fonts = &shell.fonts;
    match &row.kind {
        Kind::Field { text, preedit, focused } => {
            let r = rect(x - 4.0, top + 4.0, right - x + 8.0, ROW - 8.0);
            rounded(pm, r, 7.0, if *focused { LIGHT.accent } else { LIGHT.separator }, 255);
            rounded(pm, rect(r.left() + 1.5, r.top() + 1.5, r.width() - 3.0, r.height() - 3.0), 5.5, WHITE, 255);
            let shown = format!("{text}{preedit}");
            let placeholder = shown.is_empty() && !*focused;
            let mut tail = if placeholder { row.title.clone() } else { shown };
            while fonts.width(Weight::Regular, 12.0, &tail) > r.width() - 16.0 && !tail.is_empty() {
                tail.remove(0);
            }
            let color = if placeholder { LIGHT.subtext } else { LIGHT.text };
            fonts.draw(pm, Weight::Regular, 12.0, x + 2.0, top + 21.5, color, &tail);
            if *focused {
                let cx = x + 3.0 + fonts.width(Weight::Regular, 12.0, &tail);
                fill(pm, rect(cx, top + 10.0, 1.5, 14.0), LIGHT.accent, 255);
            }
            return;
        }
        Kind::Slider(f) | Kind::Bar(f) => {
            fonts.draw(pm, Weight::Medium, 11.5, x, top + 14.0, LIGHT.text, &row.title);
            fonts.draw_right(pm, Weight::Regular, 10.0, right, top + 14.0, LIGHT.subtext, &row.value);
            let width = right - x;
            rounded(pm, rect(x, top + 22.0, width, 4.0), 2.0, LIGHT.separator, 255);
            let filled = (width * f.clamp(0.0, 1.0)).max(4.0);
            rounded(pm, rect(x, top + 22.0, filled, 4.0), 2.0, LIGHT.accent, 255);
            if matches!(row.kind, Kind::Slider(_)) {
                rounded(pm, rect(x + filled - 6.0, top + 18.0, 12.0, 12.0), 6.0, WHITE, 255);
                rounded(pm, rect(x + filled - 4.0, top + 20.0, 8.0, 8.0), 4.0, LIGHT.accent, 255);
            }
            return;
        }
        _ => {}
    }

    let trailing = match &row.kind {
        Kind::Toggle(_) => 34.0,
        Kind::Network { .. } => 36.0,
        Kind::Link => 14.0,
        _ => 0.0,
    };
    let max = right - x - trailing;
    let color = if matches!(row.kind, Kind::Button) { LIGHT.accent } else { LIGHT.text };
    if row.value.is_empty() {
        fonts.draw(pm, Weight::Medium, 11.5, x, top + 21.0, color, &fonts.fit(Weight::Medium, 11.5, &row.title, max));
    } else {
        fonts.draw(pm, Weight::Medium, 11.5, x, top + 15.0, color, &fonts.fit(Weight::Medium, 11.5, &row.title, max));
        let value_color = if row.value == "Connected" { LIGHT.accent } else { LIGHT.subtext };
        fonts.draw(pm, Weight::Regular, 9.5, x, top + 27.0, value_color, &fonts.fit(Weight::Regular, 9.5, &row.value, max));
    }

    match &row.kind {
        Kind::Toggle(on) => {
            rounded(pm, rect(right - 28.0, top + 10.0, 28.0, 15.0), 7.5, if *on { LIGHT.accent } else { LIGHT.separator }, 255);
            let knob = if *on { right - 13.5 } else { right - 26.5 };
            rounded(pm, rect(knob, top + 11.5, 12.0, 12.0), 6.0, WHITE, 255);
        }
        Kind::Network { bars, locked } => {
            let name = match bars {
                0 => "network-wireless-signal-none-symbolic",
                1 => "network-wireless-signal-weak-symbolic",
                2 => "network-wireless-signal-ok-symbolic",
                3 => "network-wireless-signal-good-symbolic",
                _ => "network-wireless-signal-excellent-symbolic",
            };
            if let Some(img) = shell.icons.get(name, 14, Some(LIGHT.text)) {
                image(pm, img, right - 14.0, top + 10.0);
            }
            if *locked {
                if let Some(img) = shell.icons.get("changes-prevent-symbolic", 11, Some(LIGHT.subtext)) {
                    image(pm, img, right - 32.0, top + 11.5);
                }
            }
        }
        Kind::Link => {
            if let Some(img) = shell.icons.get("go-next-symbolic", 11, Some(LIGHT.subtext)) {
                image(pm, img, right - 11.0, top + 11.5);
            }
        }
        _ => {}
    }
}
