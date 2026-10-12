use std::collections::BTreeMap;
use std::path::PathBuf;

/// User settings shared by the shell tools and the idle script, stored as `key=value` lines.
pub struct Config {
    values: BTreeMap<String, String>,
}

pub const CLOCK_24H: &str = "clock_24h";
pub const DIM_AFTER: &str = "dim_after";
pub const OFF_AFTER: &str = "off_after";

pub const DEFAULT_DIM_AFTER: u32 = 20;
pub const DEFAULT_OFF_AFTER: u32 = 30;

fn path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("agm/settings")
}

impl Config {
    pub fn load() -> Config {
        let text = std::fs::read_to_string(path()).unwrap_or_default();
        let values = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        Config { values }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        let text: String = self.values.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        std::fs::write(path, text)
    }

    pub fn get_u32(&self, key: &str, default: u32) -> u32 {
        self.values.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.values.get(key).map_or(default, |v| v == "true")
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.to_string(), value.to_string());
    }
}
