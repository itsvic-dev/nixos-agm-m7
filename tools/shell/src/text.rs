use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use tiny_skia::{Pixmap, PremultipliedColorU8};

use crate::theme::Rgb;

#[derive(Clone, Copy)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
    /// Light at the display optical size, for the big clock.
    Display,
}

pub struct Fonts {
    regular: FontArc,
    medium: FontArc,
    semibold: FontArc,
    display: FontArc,
}

fn load(path: &str) -> FontArc {
    FontArc::try_from_vec(std::fs::read(path).expect(path)).expect(path)
}

impl Fonts {
    pub fn load() -> Fonts {
        let dir = env!("AGM_FONT_DIR");
        Fonts {
            regular: load(&format!("{dir}/Inter-Regular.ttf")),
            medium: load(&format!("{dir}/Inter-Medium.ttf")),
            semibold: load(&format!("{dir}/Inter-SemiBold.ttf")),
            display: load(&format!("{dir}/Inter-Display-Light.ttf")),
        }
    }

    fn font(&self, weight: Weight) -> &FontArc {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Medium => &self.medium,
            Weight::SemiBold => &self.semibold,
            Weight::Display => &self.display,
        }
    }

    pub fn width(&self, weight: Weight, px: f32, s: &str) -> f32 {
        let font = self.font(weight).as_scaled(PxScale::from(px));
        let mut width = 0.0;
        let mut prev = None;
        for c in s.chars() {
            let id = font.glyph_id(c);
            if let Some(p) = prev {
                width += font.kern(p, id);
            }
            width += font.h_advance(id);
            prev = Some(id);
        }
        width
    }

    /// Shortens `s` with an ellipsis until it fits in `max` pixels.
    pub fn fit(&self, weight: Weight, px: f32, s: &str, max: f32) -> String {
        if self.width(weight, px, s) <= max {
            return s.to_string();
        }
        let mut out = s.to_string();
        while !out.is_empty() && self.width(weight, px, &format!("{out}…")) > max {
            out.pop();
        }
        format!("{}…", out.trim_end())
    }

    /// Draws `s` with its left edge at `x` and its baseline at `base`.
    pub fn draw(&self, pm: &mut Pixmap, weight: Weight, px: f32, x: f32, base: f32, color: Rgb, s: &str) {
        let font = self.font(weight);
        let scale = PxScale::from(px);
        let scaled = font.as_scaled(scale);
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        let mut caret = x;
        let mut prev = None;
        for c in s.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                caret += scaled.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(caret, base));
            caret += scaled.h_advance(id);
            prev = Some(id);
            let Some(outline) = font.outline_glyph(glyph) else { continue };
            let bounds = outline.px_bounds();
            let pixels = pm.pixels_mut();
            outline.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && px < w && py < h {
                    let i = (py * w + px) as usize;
                    pixels[i] = blend(pixels[i], color, coverage.min(1.0));
                }
            });
        }
    }

    pub fn draw_centered(&self, pm: &mut Pixmap, weight: Weight, px: f32, cx: f32, base: f32, color: Rgb, s: &str) {
        let x = (cx - self.width(weight, px, s) / 2.0).round();
        self.draw(pm, weight, px, x, base, color, s);
    }

    pub fn draw_right(&self, pm: &mut Pixmap, weight: Weight, px: f32, right: f32, base: f32, color: Rgb, s: &str) {
        let x = (right - self.width(weight, px, s)).round();
        self.draw(pm, weight, px, x, base, color, s);
    }
}

pub fn blend(dst: PremultipliedColorU8, src: Rgb, a: f32) -> PremultipliedColorU8 {
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
