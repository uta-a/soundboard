use cpal::traits::{DeviceTrait, StreamTrait};
use rodio::Source;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// 200ms相当を超えて溜まったら古いサンプルを破棄し、レイテンシの蓄積を防ぐ。
const MAX_BUFFERED_MS: u64 = 200;

pub struct MicControl {
    enabled: AtomicBool,
    volume_bits: AtomicU32,
}

impl MicControl {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(false),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
        })
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

struct RingBuffer {
    data: Mutex<VecDeque<f32>>,
    max_len: usize,
}

pub struct MicSource {
    buffer: Arc<RingBuffer>,
    control: Arc<MicControl>,
    channels: u16,
    sample_rate: u32,
}

impl Iterator for MicSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.buffer.data.lock().unwrap().pop_front().unwrap_or(0.0);
        if self.control.is_enabled() {
            Some(sample * self.control.volume())
        } else {
            Some(0.0)
        }
    }
}

impl Source for MicSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct MicCapture {
    _stream: cpal::Stream,
    pub control: Arc<MicControl>,
    buffer: Arc<RingBuffer>,
    channels: u16,
    sample_rate: u32,
}

// cpal::Stream はWindows(WASAPI)実装ではCOM由来のポインタを保持しており
// Send を実装しない。実際のキャプチャはcpalが生成する専用スレッドで行われ、
// このアプリではストリームをMutexで排他アクセスするのみなので、
// tauri::State に載せるために Send/Sync を明示的に付与する。
unsafe impl Send for MicCapture {}
unsafe impl Sync for MicCapture {}

impl MicCapture {
    /// このキャプチャの音声を出力する新しい MicSource を作る。
    /// 出力デバイス切り替え時など、同じキャプチャを複数の OutputStreamHandle に
    /// 再登録したい場合に使う。
    pub fn make_source(&self) -> MicSource {
        MicSource {
            buffer: self.buffer.clone(),
            control: self.control.clone(),
            channels: self.channels,
            sample_rate: self.sample_rate,
        }
    }
}

pub fn start_capture(device: &cpal::Device) -> Result<MicCapture, String> {
    let config = device
        .default_input_config()
        .map_err(|e| format!("マイクの設定取得に失敗しました: {e}"))?;
    let channels = config.channels();
    let sample_rate = config.sample_rate().0;

    let max_len = (sample_rate as u64 * channels as u64 * MAX_BUFFERED_MS / 1000) as usize;
    let buffer = Arc::new(RingBuffer {
        data: Mutex::new(VecDeque::with_capacity(max_len)),
        max_len,
    });
    let control = MicControl::new();

    let callback_buffer = buffer.clone();
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                let mut queue = callback_buffer.data.lock().unwrap();
                queue.extend(data.iter().copied());
                while queue.len() > callback_buffer.max_len {
                    queue.pop_front();
                }
            },
            |err| eprintln!("マイク入力エラー: {err}"),
            None,
        )
        .map_err(|e| format!("マイク入力ストリームの作成に失敗しました: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("マイク入力の開始に失敗しました: {e}"))?;

    Ok(MicCapture {
        _stream: stream,
        control,
        buffer,
        channels,
        sample_rate,
    })
}
