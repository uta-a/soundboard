#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod mic;
mod opus_decoder;

use audio::AudioEngine;
use config::{load_config, save_config, AppConfig, SoundEntry};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

struct AppState {
    engine: AudioEngine,
    config: Mutex<AppConfig>,
}

#[derive(Serialize)]
struct InitData {
    sounds: Vec<SoundEntry>,
    output_devices: Vec<String>,
    input_devices: Vec<String>,
    monitor_device: Option<String>,
    virtual_device: Option<String>,
    mic_device: Option<String>,
    mic_volume: f32,
    master_volume: f32,
    mic_toggle_shortcut: Option<String>,
}

#[tauri::command]
fn get_init_data(state: State<AppState>) -> Result<InitData, String> {
    let output_devices = AudioEngine::list_output_devices()?;
    let input_devices = AudioEngine::list_input_devices()?;
    let config = state.config.lock().unwrap();
    Ok(InitData {
        sounds: config.sounds.clone(),
        output_devices,
        input_devices,
        monitor_device: config.monitor_device.clone(),
        virtual_device: config.virtual_device.clone(),
        mic_device: config.mic_device.clone(),
        mic_volume: config.mic_volume,
        master_volume: config.master_volume,
        mic_toggle_shortcut: config.mic_toggle_shortcut.clone(),
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

#[tauri::command]
fn set_mic_device(app: AppHandle, state: State<AppState>, device: Option<String>) -> Result<(), String> {
    state.engine.set_mic_device(device.clone())?;

    let mut config = state.config.lock().unwrap();
    config.mic_device = device;
    save_config(&app, &config)?;
    Ok(())
}

#[tauri::command]
fn set_mic_enabled(state: State<AppState>, enabled: bool) {
    state.engine.set_mic_enabled(enabled);
}

#[tauri::command]
fn set_mic_volume(app: AppHandle, state: State<AppState>, volume: f32) -> Result<(), String> {
    state.engine.set_mic_volume(volume);

    let mut config = state.config.lock().unwrap();
    config.mic_volume = volume;
    save_config(&app, &config)?;
    Ok(())
}

#[tauri::command]
fn set_master_volume(app: AppHandle, state: State<AppState>, volume: f32) -> Result<(), String> {
    state.engine.set_master_volume(volume);

    let mut config = state.config.lock().unwrap();
    config.master_volume = volume;
    save_config(&app, &config)?;
    Ok(())
}

/// マイクON/OFFトグル用のグローバルショートカットを登録する。
/// 押下(Pressed)時のみマイクを反転し、結果をフロントへ通知する。
fn register_mic_toggle_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                let state = app.state::<AppState>();
                let enabled = state.engine.toggle_mic_enabled();
                let _ = app.emit("mic-toggled", enabled);
            }
        })
        .map_err(|e| format!("ホットキーの登録に失敗しました。他のアプリで使用中の可能性があります: {e}"))
}

#[tauri::command]
fn set_mic_toggle_shortcut(
    app: AppHandle,
    state: State<AppState>,
    shortcut: Option<String>,
) -> Result<(), String> {
    {
        let config = state.config.lock().unwrap();
        if let Some(old) = config.mic_toggle_shortcut.as_ref() {
            let _ = app.global_shortcut().unregister(old.as_str());
        }
    }

    if let Some(new_shortcut) = shortcut.as_ref() {
        register_mic_toggle_shortcut(&app, new_shortcut)?;
    }

    let mut config = state.config.lock().unwrap();
    config.mic_toggle_shortcut = shortcut;
    save_config(&app, &config)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let config = load_config(&handle).unwrap_or_default();

            let engine = AudioEngine::default();
            // 前回保存されたデバイス・音量設定への復元はベストエフォート。
            // デバイスが存在しない場合は無視して未選択のまま起動する。
            // マイクのON/OFFトグルは意図せぬ集音を避けるため、
            // 起動時は必ずOFFから始まる(set_mic_enabledは呼ばない)。
            let _ = engine.set_devices(config.monitor_device.clone(), config.virtual_device.clone());
            let _ = engine.set_mic_device(config.mic_device.clone());
            engine.set_mic_volume(config.mic_volume);
            engine.set_master_volume(config.master_volume);

            if let Some(shortcut) = config.mic_toggle_shortcut.as_ref() {
                let _ = register_mic_toggle_shortcut(&handle, shortcut);
            }

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
            set_mic_device,
            set_mic_enabled,
            set_mic_volume,
            set_master_volume,
            set_mic_toggle_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("Tauriアプリの起動に失敗しました");
}
