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

#[derive(Default)]
pub struct AudioEngine {
    monitor: Mutex<Option<OutputTarget>>,
    virtual_out: Mutex<Option<OutputTarget>>,
    sinks: Mutex<Vec<Sink>>,
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

    pub fn set_devices(&self, monitor: Option<String>, virtual_out: Option<String>) -> Result<(), String> {
        {
            let mut slot = self.monitor.lock().unwrap();
            *slot = match monitor {
                Some(name) => Some(build_target(&name)?),
                None => None,
            };
        }
        {
            let mut slot = self.virtual_out.lock().unwrap();
            *slot = match virtual_out {
                Some(name) => Some(build_target(&name)?),
                None => None,
            };
        }
        Ok(())
    }

    pub fn play_sound(&self, path: &str) -> Result<(), String> {
        let audio = decode_file(path)?;

        let mut new_sinks = Vec::new();

        for target_lock in [&self.monitor, &self.virtual_out] {
            let target = target_lock.lock().unwrap();
            if let Some(target) = target.as_ref() {
                let sink = Sink::try_new(&target.handle)
                    .map_err(|e| format!("再生キューの作成に失敗しました: {e}"))?;
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
