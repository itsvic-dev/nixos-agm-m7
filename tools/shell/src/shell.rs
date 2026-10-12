use chrono::{DateTime, Local};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::config::{self, Config};
use crate::draw::{fill, image, rect, rounded};
use crate::icons::Icons;
use crate::status::Status;
use crate::text::{Fonts, Weight};
use crate::theme::{BLACK, DARK, GREEN, Palette, RED, WHITE};

pub const BAR: f32 = 16.0;
pub const SOFT_KEY_BAR: f32 = 22.0;

/// Fonts, icons and the wallpaper, shared by the shell surfaces and apps.
pub struct Shell {
    pub fonts: Fonts,
    pub icons: Icons,
    wallpaper: Option<Pixmap>,
}

pub fn wifi_icon(status: &Status) -> &'static str {
    match status.wifi.as_ref().map(|w| w.bars) {
        None => "network-wireless-offline-symbolic",
        Some(0) => "network-wireless-signal-none-symbolic",
        Some(1) => "network-wireless-signal-weak-symbolic",
        Some(2) => "network-wireless-signal-ok-symbolic",
        Some(3) => "network-wireless-signal-good-symbolic",
        Some(_) => "network-wireless-signal-excellent-symbolic",
    }
}

pub fn battery_icon(status: &Status) -> String {
    match &status.battery {
        None => "battery-missing-symbolic".into(),
        Some(b) if b.charging && b.percent >= 100 => "battery-level-100-charged-symbolic".into(),
        Some(b) => {
            let level = (b.percent.min(100) / 10) * 10;
            format!("battery-level-{level}{}-symbolic", if b.charging { "-charging" } else { "" })
        }
    }
}

impl Shell {
    pub fn load() -> Shell {
        Shell { fonts: Fonts::load(), icons: Icons::new(), wallpaper: Pixmap::load_png(env!("AGM_WALLPAPER")).ok() }
    }

    pub fn wallpaper(&self, pm: &mut Pixmap) {
        pm.fill(crate::draw::rgba(DARK.bg, 255));
        if let Some(wp) = &self.wallpaper {
            let (w, h) = (pm.width() as f32, pm.height() as f32);
            let scale = (w / wp.width() as f32).max(h / wp.height() as f32);
            let dx = (w - wp.width() as f32 * scale) / 2.0;
            let dy = (h - wp.height() as f32 * scale) / 2.0;
            let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
            pm.draw_pixmap(0, 0, wp.as_ref(), &paint, Transform::from_row(scale, 0.0, 0.0, scale, dx, dy), None);
        }
    }

    pub fn time(now: DateTime<Local>) -> String {
        let h24 = Config::load().get_bool(config::CLOCK_24H, true);
        now.format(if h24 { "%H:%M" } else { "%-I:%M" }).to_string()
    }

    /// The black status bar: Wi-Fi on the left, the time in the middle, the battery on the right.
    pub fn status_bar(&mut self, pm: &mut Pixmap, status: &Status, now: DateTime<Local>) {
        let w = pm.width() as f32;
        fill(pm, rect(0.0, 0.0, w, BAR), BLACK, 255);
        let tint = if status.wifi.is_some() { WHITE } else { DARK.subtext };
        if let Some(icon) = self.icons.get(wifi_icon(status), 12, Some(tint)) {
            image(pm, icon, 6.0, 2.0);
        }
        self.fonts.draw_centered(pm, Weight::SemiBold, 10.5, w / 2.0, 12.0, WHITE, &Self::time(now));
        let tint = match &status.battery {
            Some(b) if b.charging => GREEN,
            Some(b) if b.percent <= 15 => RED,
            _ => WHITE,
        };
        if let Some(icon) = self.icons.get(&battery_icon(status), 14, Some(tint)) {
            image(pm, icon, w - 20.0, 1.0);
        }
    }

    /// The clock face of the home screen and the keyguard, with its top at `top`.
    pub fn clock_face(&mut self, pm: &mut Pixmap, top: f32, status: &Status, now: DateTime<Local>, locked: bool) {
        let w = pm.width() as f32;
        let mut y = top + 30.0;
        if locked {
            if let Some(icon) = self.icons.get("system-lock-screen-symbolic", 14, Some(DARK.subtext)) {
                image(pm, icon, (w - 14.0) / 2.0, top + 10.0);
            }
            y += 6.0;
        }
        let h24 = Config::load().get_bool(config::CLOCK_24H, true);
        self.fonts.draw_centered(pm, Weight::Display, 64.0, w / 2.0, y + 50.0, DARK.text, &Self::time(now));
        let date = now.format(if h24 { "%A, %-d %B" } else { "%A, %-d %B · %p" }).to_string();
        self.fonts.draw_centered(pm, Weight::Regular, 12.0, w / 2.0, y + 72.0, DARK.subtext, &date);

        if !locked {
            let (label, tint) = match &status.wifi {
                Some(wifi) => (self.fonts.fit(Weight::Medium, 10.0, &wifi.name, 150.0), DARK.accent),
                None => ("No Wi-Fi".to_string(), DARK.subtext),
            };
            let tw = self.fonts.width(Weight::Medium, 10.0, &label);
            let cw = tw + 30.0;
            let cx = ((w - cw) / 2.0).round();
            rounded(pm, rect(cx, y + 86.0, cw, 18.0), 9.0, WHITE, 20);
            if let Some(icon) = self.icons.get(wifi_icon(status), 11, Some(tint)) {
                image(pm, icon, cx + 8.0, y + 89.5);
            }
            self.fonts.draw(pm, Weight::Medium, 10.0, cx + 22.0, y + 98.5, DARK.text, &label);
        }
    }

    /// Soft key labels. Dark surfaces get a soft shade, light ones a white bar with accent labels.
    pub fn soft_keys(&self, pm: &mut Pixmap, palette: &Palette, left: &str, right: &str) {
        let (w, h) = (pm.width() as f32, pm.height() as f32);
        let top = h - SOFT_KEY_BAR;
        let color = if std::ptr::eq(palette, &DARK) {
            fill(pm, rect(0.0, top, w, SOFT_KEY_BAR), BLACK, 70);
            palette.text
        } else {
            fill(pm, rect(0.0, top, w, SOFT_KEY_BAR), palette.surface, 255);
            fill(pm, rect(0.0, top, w, 1.0), palette.separator, 255);
            palette.accent
        };
        self.fonts.draw(pm, Weight::Medium, 11.0, 10.0, h - 7.5, color, left);
        self.fonts.draw_right(pm, Weight::Medium, 11.0, w - 10.0, h - 7.5, color, right);
    }

    pub fn hint(&self, pm: &mut Pixmap, text: &str) {
        let (w, h) = (pm.width() as f32, pm.height() as f32);
        self.fonts.draw_centered(pm, Weight::Regular, 11.0, w / 2.0, h - SOFT_KEY_BAR - 12.0, DARK.subtext, text);
    }
}
