pub mod config;
pub mod draw;
pub mod icons;
pub mod shell;
pub mod status;
pub mod text;
pub mod theme;
pub mod wl;

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
