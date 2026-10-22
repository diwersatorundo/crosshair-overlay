use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Self::opaque(0, 255, 0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub style: u8,
    pub follow_recoil: bool,
    pub center_dot: bool,
    pub t_style: bool,
    pub outline: u8,
    pub color: Rgba,
    pub outline_color: Rgba,
    pub gap: i32,
    pub length: u32,
    pub thickness: u32,
    pub screen_height: u32,
    pub scale_with_screen: bool,
    pub offset_x: i32,
    pub offset_y: i32,
    pub dynamic_spread_limit: u32,
    pub split_distance: u32,
    pub inner_split_alpha: f32,
    pub outer_split_alpha: f32,
    pub split_size_ratio: f32,
    pub scope_dot_scale: f32,
    pub scope_dot_color_match: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            style: 4,
            follow_recoil: false,
            center_dot: false,
            t_style: false,
            outline: 1,
            color: Rgba::opaque(0, 255, 0),
            outline_color: Rgba::new(0, 0, 0, 255),
            gap: -1,
            length: 5,
            thickness: 1,
            screen_height: 1080,
            scale_with_screen: true,
            offset_x: 0,
            offset_y: 0,
            dynamic_spread_limit: 181,
            split_distance: 3,
            inner_split_alpha: 1.0,
            outer_split_alpha: 0.35,
            split_size_ratio: 0.0,
            scope_dot_scale: 1.0,
            scope_dot_color_match: false,
        }
    }
}

impl Config {
    pub fn sanitized(mut self) -> Self {
        self.style = self.style.min(9);
        self.outline = self.outline.min(2);
        self.gap = self.gap.clamp(-3840, 3840);
        self.length = self.length.min(255);
        self.thickness = self.thickness.min(32);
        self.screen_height = self.screen_height.clamp(240, 65535);
        self.offset_x = self.offset_x.clamp(-16384, 16384);
        self.offset_y = self.offset_y.clamp(-16384, 16384);
        self.dynamic_spread_limit = self.dynamic_spread_limit.min(255);
        self.split_distance = self.split_distance.min(127);
        self.inner_split_alpha = clamp01(self.inner_split_alpha);
        self.outer_split_alpha = self.outer_split_alpha.clamp(0.3, 1.0);
        self.split_size_ratio = clamp01(self.split_size_ratio);
        if !self.scope_dot_scale.is_finite() {
            self.scope_dot_scale = 1.0;
        }
        self.scope_dot_scale = self.scope_dot_scale.clamp(0.1, 2.0);
        self
    }
}

fn clamp01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("CrosshairOverlay")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load() -> (Config, Option<String>) {
    match fs::read_to_string(config_path()) {
        Ok(text) => match serde_json::from_str::<Config>(&text) {
            Ok(cfg) => (cfg.sanitized(), None),
            Err(err) => (
                Config::default(),
                Some(format!(
                    "Не удалось прочитать config.json: {err}. Загружены настройки по умолчанию."
                )),
            ),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => (Config::default(), None),
        Err(err) => (
            Config::default(),
            Some(format!(
                "Не удалось открыть config.json: {err}. Загружены настройки по умолчанию."
            )),
        ),
    }
}

pub fn save(cfg: &Config) -> io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.json");
    let tmp = dir.join("config.json.tmp");
    let json = serde_json::to_vec_pretty(cfg).map_err(io::Error::other)?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_fields() {
        let cfg = Config {
            style: 200,
            outline: 9,
            gap: -9000,
            length: 999,
            thickness: 99,
            screen_height: 10,
            inner_split_alpha: 42.0,
            outer_split_alpha: -1.0,
            split_size_ratio: f32::NAN,
            scope_dot_scale: 12.0,
            ..Config::default()
        }
        .sanitized();
        assert_eq!(cfg.style, 9);
        assert_eq!(cfg.outline, 2);
        assert_eq!(cfg.gap, -3840);
        assert_eq!(cfg.length, 255);
        assert_eq!(cfg.thickness, 32);
        assert_eq!(cfg.screen_height, 240);
        assert_eq!(cfg.inner_split_alpha, 1.0);
        assert_eq!(cfg.outer_split_alpha, 0.3);
        assert_eq!(cfg.split_size_ratio, 0.0);
        assert_eq!(cfg.scope_dot_scale, 2.0);
    }

    #[test]
    fn missing_fields_get_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"gap": 4}"#).unwrap();
        assert_eq!(cfg.gap, 4);
        assert_eq!(cfg.style, Config::default().style);
        assert_eq!(cfg.color, Config::default().color);
    }
}
