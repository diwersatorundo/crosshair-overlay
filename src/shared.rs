use crate::logging;
use crosshair_core::config::{self, Config};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub config: Config,
    pub version: u64,
    pub overlay_running: bool,
    pub overlay_error: Option<String>,
    pub hotkey_error: Option<String>,
}

#[derive(Debug)]
struct Inner {
    config: Config,
    version: u64,
    overlay_running: bool,
    overlay_error: Option<String>,
    hotkey_error: Option<String>,
}

pub struct Shared {
    inner: Mutex<Inner>,
    save_lock: Mutex<()>,
}

impl Shared {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                config,
                version: 1,
                overlay_running: false,
                overlay_error: None,
                hotkey_error: None,
            }),
            save_lock: Mutex::new(()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn snapshot(&self) -> Snapshot {
        let inner = self.lock();
        Snapshot {
            config: inner.config.clone(),
            version: inner.version,
            overlay_running: inner.overlay_running,
            overlay_error: inner.overlay_error.clone(),
            hotkey_error: inner.hotkey_error.clone(),
        }
    }

    pub fn set_config(&self, config: Config) -> u64 {
        let config = config.sanitized();
        let mut inner = self.lock();
        inner.config = config;
        inner.version = inner.version.wrapping_add(1).max(1);
        inner.version
    }

    pub fn toggle_enabled(&self) -> (Config, u64) {
        let mut inner = self.lock();
        inner.config.enabled = !inner.config.enabled;
        inner.version = inner.version.wrapping_add(1).max(1);
        (inner.config.clone(), inner.version)
    }

    pub fn set_overlay_running(&self, running: bool) {
        self.lock().overlay_running = running;
    }

    pub fn set_overlay_error(&self, error: Option<String>) {
        self.lock().overlay_error = error;
    }

    pub fn set_hotkey_error(&self, error: Option<String>) {
        self.lock().hotkey_error = error;
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
        let _guard = self.save_lock.lock().unwrap_or_else(|p| p.into_inner());
        config::save(config).map_err(|err| {
            let message = format!("Не удалось сохранить настройки: {err}");
            logging::log("ERROR", &message);
            message
        })
    }
}
