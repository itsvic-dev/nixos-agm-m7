use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tiny_skia::{Pixmap, PremultipliedColorU8, Transform};

use crate::theme::Rgb;

/// Freedesktop icon lookup by name in the hicolor and Adwaita themes, with a cache of rendered sizes.
pub struct Icons {
    index: HashMap<String, PathBuf>,
    cache: HashMap<(String, u32, Option<Rgb>), Option<Pixmap>>,
}

pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let data_home = std::env::var("XDG_DATA_HOME").unwrap_or(format!("{home}/.local/share"));
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or("/run/current-system/sw/share".into());
    std::iter::once(data_home.as_str())
        .chain(dirs.split(':'))
        .chain(std::iter::once(env!("AGM_ICON_DIR")))
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Size rank of an icon theme subdirectory: scalable and symbolic first, then larger bitmaps.
fn rank(dir: &str) -> u32 {
    match dir {
        "scalable" | "symbolic" => 10_000,
        d => d.split('x').next().and_then(|n| n.parse().ok()).unwrap_or(0),
    }
}

impl Icons {
    pub fn new() -> Icons {
        let mut index: HashMap<String, (u32, PathBuf)> = HashMap::new();
        for dir in data_dirs() {
            let pixmaps = dir.join("pixmaps");
            Self::scan(&pixmaps, 1, &mut index);
            for theme in ["Adwaita", "hicolor"] {
                let Ok(sizes) = std::fs::read_dir(dir.join("icons").join(theme)) else { continue };
                for size in sizes.flatten() {
                    let r = rank(&size.file_name().to_string_lossy());
                    let Ok(categories) = std::fs::read_dir(size.path()) else { continue };
                    for category in categories.flatten() {
                        Self::scan(&category.path(), r, &mut index);
                    }
                }
            }
        }
        Icons { index: index.into_iter().map(|(k, (_, p))| (k, p)).collect(), cache: HashMap::new() }
    }

    fn scan(dir: &Path, rank: u32, index: &mut HashMap<String, (u32, PathBuf)>) {
        let Ok(files) = std::fs::read_dir(dir) else { return };
        for file in files.flatten() {
            let path = file.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if !matches!(ext, Some("svg" | "png")) {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let better = index.get(name).is_none_or(|(r, _)| rank > *r);
            if better {
                index.insert(name.to_string(), (rank, path));
            }
        }
    }

    /// Renders `name` (an icon name or an absolute path) into a `size` square, recolored to `tint` if set.
    pub fn get(&mut self, name: &str, size: u32, tint: Option<Rgb>) -> Option<&Pixmap> {
        let key = (name.to_string(), size, tint);
        if !self.cache.contains_key(&key) {
            let path = if name.starts_with('/') { Some(PathBuf::from(name)) } else { self.index.get(name).cloned() };
            let icon = path.and_then(|p| render(&p, size)).map(|mut pm| {
                if let Some(t) = tint {
                    for p in pm.pixels_mut() {
                        let a = p.alpha() as u16;
                        let c = |v: u8| ((v as u16 * a) / 255) as u8;
                        *p = PremultipliedColorU8::from_rgba(c(t[0]), c(t[1]), c(t[2]), p.alpha()).unwrap();
                    }
                }
                pm
            });
            self.cache.insert(key.clone(), icon);
        }
        self.cache.get(&key).and_then(|p| p.as_ref())
    }
}

fn render(path: &Path, size: u32) -> Option<Pixmap> {
    let mut out = Pixmap::new(size, size)?;
    if path.extension().and_then(|e| e.to_str()) == Some("svg") {
        let data = std::fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let s = tree.size();
        let scale = size as f32 / s.width().max(s.height());
        let dx = (size as f32 - s.width() * scale) / 2.0;
        let dy = (size as f32 - s.height() * scale) / 2.0;
        resvg::render(&tree, Transform::from_row(scale, 0.0, 0.0, scale, dx, dy), &mut out.as_mut());
    } else {
        let png = Pixmap::load_png(path).ok()?;
        let scale = size as f32 / png.width().max(png.height()) as f32;
        let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
        out.draw_pixmap(0, 0, png.as_ref(), &paint, Transform::from_scale(scale, scale), None);
    }
    Some(out)
}
