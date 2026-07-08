use crate::mic::{self, MicCapture};
use crate::opus_decoder;
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{buffer::SamplesBuffer, Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::Cursor;
use std::sync::Mutex;

struct OutputTarget {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

// rodio::OutputStream は内部で cpal の Windows(WASAPI) ストリームを保持しており、
// COM由来のポインタが含まれるため Send を実装しない。
// 実際の再生は cpal が生成する専用オーディオスレッドで行われ、
// このアプリではストリーム自体を Mutex で排他アクセスするのみなので、
// tauri::State に載せるために Send/Sync を明示的に付与する。
unsafe impl Send for OutputTarget {}
unsafe impl Sync for OutputTarget {}

pub struct AudioEngine {
    monitor: Mutex<Option<OutputTarget>>,
    virtual_out: Mutex<Option<OutputTarget>>,
    sinks: Mutex<Vec<Sink>>,
    mic: Mutex<Option<MicCapture>>,
    master_volume: Mutex<f32>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self {
            monitor: Mutex::new(None),
            virtual_out: Mutex::new(None),
            sinks: Mutex::new(Vec::new()),
            mic: Mutex::new(None),
            master_volume: Mutex::new(1.0),
        }
    }
}

pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

fn decode_file(path: &str) -> Result<DecodedAudio, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("音声ファイルを開けませんでした: {e}"))?;

    // rodio(symphonia)はOpusコーデックに対応していないため、
    // OGG Opusのみ専用デコーダーへ振り分ける。
    if opus_decoder::is_ogg_opus(&bytes) {
        return opus_decoder::decode(&bytes);
    }

    let decoder =
        Decoder::new(Cursor::new(bytes)).map_err(|e| format!("音声のデコードに失敗しました: {e}"))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.convert_samples().collect();
    Ok(DecodedAudio {
        samples,
        channels,
        sample_rate,
    })
}

fn find_device(name: &str) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    host.output_devices()
        .map_err(|e| format!("出力デバイスの列挙に失敗しました: {e}"))?
        .find(|d| d.name().map(|n| n == name).unwrap_or(false))
        .ok_or_else(|| format!("出力デバイスが見つかりません: {name}"))
}

fn find_input_device(name: &str) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    host.input_devices()
        .map_err(|e| format!("入力デバイスの列挙に失敗しました: {e}"))?
        .find(|d| d.name().map(|n| n == name).unwrap_or(false))
        .ok_or_else(|| format!("入力デバイスが見つかりません: {name}"))
}

fn build_target(name: &str) -> Result<OutputTarget, String> {
    let device = find_device(name)?;
    let (stream, handle) = OutputStream::try_from_device(&device)
        .map_err(|e| format!("出力ストリームの作成に失敗しました: {e}"))?;
    Ok(OutputTarget {
        _stream: stream,
        handle,
    })
}

impl AudioEngine {
    pub fn list_output_devices() -> Result<Vec<String>, String> {
        let host = cpal::default_host();
        let devices = host
            .output_devices()
            .map_err(|e| format!("出力デバイスの列挙に失敗しました: {e}"))?;
        Ok(devices.filter_map(|d| d.name().ok()).collect())
    }

    pub fn list_input_devices() -> Result<Vec<String>, String> {
        let host = cpal::default_host();
        let devices = host
            .input_devices()
            .map_err(|e| format!("入力デバイスの列挙に失敗しました: {e}"))?;
        Ok(devices.filter_map(|d| d.name().ok()).collect())
    }

    pub fn set_devices(&self, monitor: Option<String>, virtual_out: Option<String>) -> Result<(), String> {
        {
            let mut slot = self.monitor.lock().unwrap();
            *slot = match monitor {
                Some(name) => Some(build_target(&name)?),
                None => None,
            };
        }
        {
            let new_virtual = match virtual_out {
                Some(name) => Some(build_target(&name)?),
                None => None,
            };
            if let Some(target) = new_virtual.as_ref() {
                self.attach_mic(target)?;
            }
            let mut slot = self.virtual_out.lock().unwrap();
            *slot = new_virtual;
        }
        Ok(())
    }

    /// 現在設定されているマイクキャプチャを、指定した仮想デバイスの
    /// OutputStreamHandle に(再)登録する。マイクは出力デバイスの切り替え
    /// ごとにハンドルが作り直されるため、都度呼び直す必要がある。
    fn attach_mic(&self, target: &OutputTarget) -> Result<(), String> {
        let mic = self.mic.lock().unwrap();
        if let Some(capture) = mic.as_ref() {
            target
                .handle
                .play_raw(capture.make_source())
                .map_err(|e| format!("マイクの登録に失敗しました: {e}"))?;
        }
        Ok(())
    }

    pub fn set_mic_device(&self, name: Option<String>) -> Result<(), String> {
        match name {
            None => {
                *self.mic.lock().unwrap() = None;
            }
            Some(name) => {
                let device = find_input_device(&name)?;
                let capture = mic::start_capture(&device)?;
                {
                    let virtual_target = self.virtual_out.lock().unwrap();
                    if let Some(target) = virtual_target.as_ref() {
                        target
                            .handle
                            .play_raw(capture.make_source())
                            .map_err(|e| format!("マイクの登録に失敗しました: {e}"))?;
                    }
                }
                *self.mic.lock().unwrap() = Some(capture);
            }
        }
        Ok(())
    }

    pub fn set_mic_enabled(&self, enabled: bool) {
        if let Some(capture) = self.mic.lock().unwrap().as_ref() {
            capture.control.set_enabled(enabled);
        }
    }

    /// 現在のマイク有効状態を反転して返す。マイクが未設定の場合は何もせず `false` を返す。
    pub fn toggle_mic_enabled(&self) -> bool {
        if let Some(capture) = self.mic.lock().unwrap().as_ref() {
            let new_state = !capture.control.is_enabled();
            capture.control.set_enabled(new_state);
            new_state
        } else {
            false
        }
    }

    pub fn set_mic_volume(&self, volume: f32) {
        if let Some(capture) = self.mic.lock().unwrap().as_ref() {
            capture.control.set_volume(volume);
        }
    }

    pub fn set_master_volume(&self, volume: f32) {
        *self.master_volume.lock().unwrap() = volume;
        let sinks = self.sinks.lock().unwrap();
        for sink in sinks.iter() {
            sink.set_volume(volume);
        }
    }

    pub fn play_sound(&self, path: &str) -> Result<(), String> {
        let audio = decode_file(path)?;
        let master_volume = *self.master_volume.lock().unwrap();

        let mut new_sinks = Vec::new();

        for target_lock in [&self.monitor, &self.virtual_out] {
            let target = target_lock.lock().unwrap();
            if let Some(target) = target.as_ref() {
                let sink = Sink::try_new(&target.handle)
                    .map_err(|e| format!("再生キューの作成に失敗しました: {e}"))?;
                sink.set_volume(master_volume);
                let buffer =
                    SamplesBuffer::new(audio.channels, audio.sample_rate, audio.samples.clone());
                sink.append(buffer);
                new_sinks.push(sink);
            }
        }

        if new_sinks.is_empty() {
            return Err("出力デバイスが選択されていません".to_string());
        }

        let mut sinks = self.sinks.lock().unwrap();
        sinks.retain(|s| !s.empty());
        sinks.extend(new_sinks);
        Ok(())
    }

    pub fn stop_all(&self) {
        let mut sinks = self.sinks.lock().unwrap();
        for sink in sinks.iter() {
            sink.stop();
        }
        sinks.clear();
    }
}
