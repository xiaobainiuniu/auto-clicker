//! 配置读写：%APPDATA%/auto-clicker/config.json，记忆上次的坐标/间隔/时长/模式。
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub point: Option<(i32, i32)>,
    pub interval_ms: u64,
    pub duration_sec: u64,
    pub mode: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            point: None,
            interval_ms: 100,
            duration_sec: 30,
            mode: "universal".to_string(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|dir| PathBuf::from(dir).join("auto-clicker").join("config.json"))
}

impl Config {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }
}
