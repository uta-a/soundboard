use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundEntry {
    pub id: String,
    pub label: String,
    pub path: String,
}

fn default_volume() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub sounds: Vec<SoundEntry>,
    #[serde(default)]
    pub monitor_device: Option<String>,
    #[serde(default)]
    pub virtual_device: Option<String>,
    #[serde(default)]
    pub mic_device: Option<String>,
    #[serde(default = "default_volume")]
    pub mic_volume: f32,
    #[serde(default = "default_volume")]
    pub master_volume: f32,
    #[serde(default)]
    pub mic_toggle_shortcut: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sounds: Vec::new(),
            monitor_device: None,
            virtual_device: None,
            mic_device: None,
            mic_volume: default_volume(),
            master_volume: default_volume(),
            mic_toggle_shortcut: None,
        }
    }
}

impl SoundEntry {
    pub fn new(label: String, path: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            label,
            path,
        }
    }
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("設定ディレクトリの取得に失敗しました: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("設定ディレクトリの作成に失敗しました: {e}"))?;
    Ok(dir.join("config.json"))
}

pub fn load_config(app: &AppHandle) -> Result<AppConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("設定の読み込みに失敗しました: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("設定の解析に失敗しました: {e}"))
}

pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let data = serde_json::to_string_pretty(config)
        .map_err(|e| format!("設定のシリアライズに失敗しました: {e}"))?;
    fs::write(&path, data).map_err(|e| format!("設定の保存に失敗しました: {e}"))
}
