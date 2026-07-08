#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod opus_decoder;

use audio::AudioEngine;
use config::{load_config, save_config, AppConfig, SoundEntry};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

struct AppState {
    engine: AudioEngine,
    config: Mutex<AppConfig>,
}

#[derive(Serialize)]
struct InitData {
    sounds: Vec<SoundEntry>,
    devices: Vec<String>,
    monitor_device: Option<String>,
    virtual_device: Option<String>,
}

#[tauri::command]
fn get_init_data(state: State<AppState>) -> Result<InitData, String> {
    let devices = AudioEngine::list_output_devices()?;
    let config = state.config.lock().unwrap();
    Ok(InitData {
        sounds: config.sounds.clone(),
        devices,
        monitor_device: config.monitor_device.clone(),
        virtual_device: config.virtual_device.clone(),
    })
}

#[tauri::command]
fn set_devices(
    app: AppHandle,
    state: State<AppState>,
    monitor: Option<String>,
    virtual_device: Option<String>,
) -> Result<(), String> {
    state.engine.set_devices(monitor.clone(), virtual_device.clone())?;

    let mut config = state.config.lock().unwrap();
    config.monitor_device = monitor;
    config.virtual_device = virtual_device;
    save_config(&app, &config)?;
    Ok(())
}

#[tauri::command]
fn play_sound(state: State<AppState>, path: String) -> Result<(), String> {
    state.engine.play_sound(&path)
}

#[tauri::command]
fn stop_all(state: State<AppState>) {
    state.engine.stop_all();
}

#[tauri::command]
fn add_sound(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    label: String,
) -> Result<SoundEntry, String> {
    let entry = SoundEntry::new(label, path);
    let mut config = state.config.lock().unwrap();
    config.sounds.push(entry.clone());
    save_config(&app, &config)?;
    Ok(entry)
}

#[tauri::command]
fn remove_sound(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.sounds.retain(|s| s.id != id);
    save_config(&app, &config)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let config = load_config(&handle).unwrap_or_default();

            let engine = AudioEngine::default();
            // 前回保存されたデバイスへの復元はベストエフォート。
            // デバイスが存在しない場合は無視して未選択のまま起動する。
            let _ = engine.set_devices(config.monitor_device.clone(), config.virtual_device.clone());

            app.manage(AppState {
                engine,
                config: Mutex::new(config),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_init_data,
            set_devices,
            play_sound,
            stop_all,
            add_sound,
            remove_sound,
        ])
        .run(tauri::generate_context!())
        .expect("Tauriアプリの起動に失敗しました");
}
