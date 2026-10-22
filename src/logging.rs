use std::time::{SystemTime, UNIX_EPOCH};

pub fn log_path() -> std::path::PathBuf {
    crosshair_core::config::config_dir().join("overlay.log")
}

pub fn log(level: &str, message: impl AsRef<str>) {
    let line = format!(
        "[{}] [{}] {}\n",
        timestamp(),
        level,
        message.as_ref().replace('\n', " ")
    );
    let _ = std::fs::create_dir_all(crosshair_core::config::config_dir());
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        log("PANIC", format!("{info} at {location}"));
    }));
}
