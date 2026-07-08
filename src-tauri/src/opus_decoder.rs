use ogg::PacketReader;
use opus::{Channels, Decoder as OpusDecoder};
use std::io::Cursor;

use crate::audio::DecodedAudio;

const OPUS_SAMPLE_RATE: u32 = 48_000;
// 120ms分のフレーム(48kHzでの最大パケット長)をステレオ分確保
const MAX_FRAME_SAMPLES: usize = 5760 * 2;

pub fn is_ogg_opus(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(64)];
    head.starts_with(b"OggS") && head.windows(8).any(|w| w == b"OpusHead")
}

pub fn decode(bytes: &[u8]) -> Result<DecodedAudio, String> {
    let mut reader = PacketReader::new(Cursor::new(bytes));

    let head_packet = reader
        .read_packet()
        .map_err(|e| format!("Opusヘッダーの読み込みに失敗しました: {e}"))?
        .ok_or_else(|| "Opusヘッダーが見つかりません".to_string())?;
    let head = &head_packet.data;
    if head.len() < 19 || &head[0..8] != b"OpusHead" {
        return Err("OpusHeadの形式が不正です".to_string());
    }

    let channel_count = head[9];
    let pre_skip = u16::from_le_bytes([head[10], head[11]]) as usize;
    let mapping_family = head[18];
    if mapping_family != 0 {
        return Err("多チャンネル(mapping family != 0)のOpusには対応していません".to_string());
    }
    let channels = match channel_count {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        n => return Err(format!("対応していないOpusチャンネル数です: {n}")),
    };

    // OpusTags(コメントヘッダー)は再生に不要なため読み飛ばす
    reader
        .read_packet()
        .map_err(|e| format!("Opusタグの読み込みに失敗しました: {e}"))?;

    let mut decoder = OpusDecoder::new(OPUS_SAMPLE_RATE, channels)
        .map_err(|e| format!("Opusデコーダーの初期化に失敗しました: {e}"))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut frame_buf = [0f32; MAX_FRAME_SAMPLES];

    while let Some(packet) = reader
        .read_packet()
        .map_err(|e| format!("Opusパケットの読み込みに失敗しました: {e}"))?
    {
        let decoded_len = decoder
            .decode_float(&packet.data, &mut frame_buf, false)
            .map_err(|e| format!("Opusのデコードに失敗しました: {e}"))?;
        samples.extend_from_slice(&frame_buf[..decoded_len * channel_count as usize]);
    }

    // Ogg Opusの仕様上、先頭の pre-skip サンプル分はデコーダーの
    // プライミング用で実際の音声には含めない。
    let skip_samples = pre_skip * channel_count as usize;
    if skip_samples < samples.len() {
        samples.drain(0..skip_samples);
    } else {
        samples.clear();
    }

    Ok(DecodedAudio {
        samples,
        channels: channel_count as u16,
        sample_rate: OPUS_SAMPLE_RATE,
    })
}
