//! パソコンにある音声ファイル（WAV・MP3・FLAC・M4A・OGG など）の読み込み。

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// 読み込める拡張子（ファイルを選ぶ画面で使う）
pub const EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "m4a", "aac", "mp4", "ogg", "oga"];

fn unsupported() -> String {
    "このファイルは読み込めませんでした。WAV・MP3・FLAC・M4A・OGG のファイルを選んでください。"
        .into()
}

struct Opened {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    sample_rate: u32,
    n_frames: Option<u64>,
}

fn open(path: &Path) -> Result<Opened, String> {
    let file = File::open(path).map_err(|e| format!("ファイルを開けませんでした（{e}）"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| unsupported())?;
    let format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(unsupported)?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.ok_or_else(unsupported)?;
    let n_frames = track.codec_params.n_frames;
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| unsupported())?;
    Ok(Opened {
        format,
        decoder,
        track_id,
        sample_rate,
        n_frames,
    })
}

/// 読んだ音（ステレオのフレーム）を受け取る関数
pub type FrameSink<'a> = &'a mut dyn FnMut(&[[f32; 2]]) -> Result<(), String>;

/// ファイルを最後まで読み、ステレオのフレームを少しずつ `on_frames` に渡す。サンプリング周波数を返す。
pub fn decode_file(path: &Path, on_frames: FrameSink) -> Result<u32, String> {
    let mut opened = open(path)?;
    let mut frames: Vec<[f32; 2]> = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match opened.format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(format!("ファイルの途中が壊れています（{e}）")),
        };
        if packet.track_id() != opened.track_id {
            continue;
        }
        let decoded = match opened.decoder.decode(&packet) {
            Ok(d) => d,
            // 一部が壊れていても、読めるところは読む
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(format!("ファイルを読めませんでした（{e}）")),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let needed = decoded.capacity() as u64;
        if buffer
            .as_ref()
            .is_none_or(|b| b.capacity() < needed as usize * channels)
        {
            buffer = Some(SampleBuffer::new(needed, spec));
        }
        let buf = buffer.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        frames.clear();
        frames.extend(buf.samples().chunks_exact(channels).map(|f| {
            let l = f[0];
            let r = if channels > 1 { f[1] } else { f[0] };
            [l, r]
        }));
        on_frames(&frames)?;
    }
    Ok(opened.sample_rate)
}

/// サンプリング周波数
pub fn sample_rate(path: &Path) -> Result<u32, String> {
    Ok(open(path)?.sample_rate)
}

/// 曲の長さ（秒）。ファイルに書かれていなければ、最後まで読んで数える。
pub fn duration(path: &Path) -> Result<f64, String> {
    let opened = open(path)?;
    if let Some(n) = opened.n_frames {
        return Ok(n as f64 / opened.sample_rate as f64);
    }
    let mut count = 0u64;
    let rate = decode_file(path, &mut |f| {
        count += f.len() as u64;
        Ok(())
    })?;
    Ok(count as f64 / rate as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wav_and_mp3() {
        let dir = std::env::temp_dir().join(format!("rec-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("in.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&wav, spec).unwrap();
        for n in 0..48_000 * 2 {
            let v = ((n as f32 * 0.05).sin() * 8000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
        assert!((duration(&wav).unwrap() - 2.0).abs() < 0.01);

        // WAV から MP3 を作って、それも読めることを確かめる
        let mp3 = dir.join("out.mp3");
        let source = super::super::export::TrackSource {
            path: wav.clone(),
            start_frame: 0,
            end_frame: 96_000,
        };
        super::super::export::render_track(
            &source,
            vec![Box::new(
                super::super::export::Mp3Sink::create(&mp3, 192).unwrap(),
            )],
            &mut |_| {},
        )
        .unwrap();
        let mut frames = 0u64;
        let rate = decode_file(&mp3, &mut |f| {
            frames += f.len() as u64;
            Ok(())
        })
        .unwrap();
        assert_eq!(rate, 44_100);

        // CD用に変換できる
        let cd = dir.join("cd.wav");
        super::super::export::render_file_for_cd(&mp3, &cd, 2.0, &mut |_| {}).unwrap();
        let reader = hound::WavReader::open(&cd).unwrap();
        assert_eq!(reader.spec().sample_rate, 44_100);
        assert_eq!(reader.duration() as u64 % 588, 0);

        // 録音と同じ形式に読み込める
        let imported = dir.join("imported.wav");
        let (r, f) = super::super::export::import_audio(&mp3, &imported).unwrap();
        assert_eq!(r, 44_100);
        assert!((f as f64 / 44_100.0 - 2.0).abs() < 0.1);
        assert!((frames as f64 / 44_100.0 - 2.0).abs() < 0.1, "{frames}");
        assert!((duration(&mp3).unwrap() - 2.0).abs() < 0.1);
    }
}
