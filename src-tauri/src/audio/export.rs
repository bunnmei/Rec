//! 曲ごとの切り出しと、WAV・MP3 への書き出し。

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use mp3lame_encoder::{Bitrate, Builder, FlushNoGap, InterleavedPcm, Quality};

use super::resample::Resampler;

pub const CD_RATE: u32 = 44_100;
/// CD の1セクタ（2352バイト）= 588 フレーム
pub const CD_SECTOR_FRAMES: u64 = 588;
/// 曲の頭と終わりにかける短いフェード（秒）。切れ目のプチッという音を防ぐ。
const FADE_SECS: f64 = 0.01;

/// 録音ファイルの中の、1曲分の範囲
#[derive(Debug, Clone)]
pub struct TrackSource {
    pub path: PathBuf,
    pub start_frame: u64,
    pub end_frame: u64,
}

pub trait Sink {
    fn write(&mut self, frames: &[[f32; 2]]) -> Result<(), String>;
    fn finish(self: Box<Self>) -> Result<(), String>;
}

fn to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn write_error(e: impl std::fmt::Display) -> String {
    format!("ファイルを保存できませんでした。保存先の空き容量を確かめてください。（{e}）")
}

/// 44.1kHz / 16bit / ステレオの WAV
pub struct WavSink {
    writer: hound::WavWriter<BufWriter<File>>,
    frames: u64,
    pad_for_cd: bool,
}

impl WavSink {
    pub fn create(path: &Path, pad_for_cd: bool) -> Result<Self, String> {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: CD_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        Ok(Self {
            writer: hound::WavWriter::create(path, spec).map_err(write_error)?,
            frames: 0,
            pad_for_cd,
        })
    }
}

impl Sink for WavSink {
    fn write(&mut self, frames: &[[f32; 2]]) -> Result<(), String> {
        for [l, r] in frames {
            self.writer.write_sample(to_i16(*l)).map_err(write_error)?;
            self.writer.write_sample(to_i16(*r)).map_err(write_error)?;
        }
        self.frames += frames.len() as u64;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        if self.pad_for_cd {
            // CD は 588 フレーム単位なので無音で埋める（曲の終わりに 1/75 秒未満の無音が付くだけ）
            let rest = (CD_SECTOR_FRAMES - self.frames % CD_SECTOR_FRAMES) % CD_SECTOR_FRAMES;
            // CD の1曲は最低4秒
            let minimum = (4 * CD_RATE as u64).saturating_sub(self.frames + rest);
            for _ in 0..(rest + minimum) * 2 {
                self.writer.write_sample(0i16).map_err(write_error)?;
            }
        }
        self.writer.finalize().map_err(write_error)
    }
}

pub struct Mp3Sink {
    encoder: mp3lame_encoder::Encoder,
    file: BufWriter<File>,
    buffer: Vec<u8>,
    pcm: Vec<i16>,
}

impl Mp3Sink {
    pub fn create(path: &Path, bitrate_kbps: u32) -> Result<Self, String> {
        let bitrate = match bitrate_kbps {
            0..=128 => Bitrate::Kbps128,
            129..=160 => Bitrate::Kbps160,
            161..=192 => Bitrate::Kbps192,
            193..=256 => Bitrate::Kbps256,
            _ => Bitrate::Kbps320,
        };
        let err = |e: mp3lame_encoder::BuildError| format!("MP3 の準備に失敗しました（{e:?}）");
        let mut builder = Builder::new().ok_or("MP3 の準備に失敗しました")?;
        builder.set_num_channels(2).map_err(err)?;
        builder.set_sample_rate(CD_RATE).map_err(err)?;
        builder.set_brate(bitrate).map_err(err)?;
        builder.set_quality(Quality::Best).map_err(err)?;
        let encoder = builder.build().map_err(err)?;
        Ok(Self {
            encoder,
            file: BufWriter::new(File::create(path).map_err(write_error)?),
            buffer: Vec::new(),
            pcm: Vec::new(),
        })
    }
}

impl Sink for Mp3Sink {
    fn write(&mut self, frames: &[[f32; 2]]) -> Result<(), String> {
        self.pcm.clear();
        self.pcm
            .extend(frames.iter().flat_map(|[l, r]| [to_i16(*l), to_i16(*r)]));
        self.buffer.clear();
        self.buffer
            .reserve(mp3lame_encoder::max_required_buffer_size(frames.len()));
        self.encoder
            .encode_to_vec(InterleavedPcm(&self.pcm), &mut self.buffer)
            .map_err(|e| format!("MP3 の作成に失敗しました（{e:?}）"))?;
        self.file.write_all(&self.buffer).map_err(write_error)
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        self.buffer.clear();
        self.buffer.reserve(7200);
        self.encoder
            .flush_to_vec::<FlushNoGap>(&mut self.buffer)
            .map_err(|e| format!("MP3 の作成に失敗しました（{e:?}）"))?;
        self.file.write_all(&self.buffer).map_err(write_error)?;
        self.file.flush().map_err(write_error)
    }
}

/// 1曲分を読み出し、44.1kHz にして各出力へ書く。`on_progress` には 0.0〜1.0 を渡す。
pub fn render_track(
    source: &TrackSource,
    mut sinks: Vec<Box<dyn Sink>>,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let mut reader = hound::WavReader::open(&source.path)
        .map_err(|e| format!("録音ファイルを開けませんでした（{e}）"))?;
    let spec = reader.spec();
    let total_frames = reader.duration() as u64;
    let start = source.start_frame.min(total_frames);
    let end = source.end_frame.min(total_frames).max(start);
    reader
        .seek(start as u32)
        .map_err(|e| format!("録音ファイルを読めませんでした（{e}）"))?;
    let length = end - start;
    let fade_frames = ((spec.sample_rate as f64 * FADE_SECS) as u64)
        .min(length / 2)
        .max(1);
    let mut resampler = Resampler::new(spec.sample_rate, CD_RATE);
    let mut samples = reader.samples::<i16>();
    let mut input = Vec::with_capacity(8192);
    let mut output = Vec::with_capacity(8192);
    let mut position = 0u64;
    while position < length {
        input.clear();
        let chunk = (length - position).min(8192);
        for i in 0..chunk {
            let mut frame = [0f32; 2];
            for slot in frame.iter_mut() {
                let s = samples
                    .next()
                    .transpose()
                    .map_err(|e| format!("録音ファイルを読めませんでした（{e}）"))?
                    .unwrap_or(0);
                *slot = s as f32 / 32768.0;
            }
            let n = position + i;
            let gain = (n.min(length - 1 - n) as f64 / fade_frames as f64).min(1.0) as f32;
            input.push([frame[0] * gain, frame[1] * gain]);
        }
        position += chunk;
        output.clear();
        resampler.process(&input, &mut output);
        for sink in sinks.iter_mut() {
            sink.write(&output)?;
        }
        on_progress(position as f64 / length.max(1) as f64);
    }
    output.clear();
    resampler.finish(&mut output);
    for sink in sinks.iter_mut() {
        sink.write(&output)?;
    }
    for sink in sinks {
        sink.finish()?;
    }
    Ok(())
}

/// 読み込んだ音声ファイルを、録音と同じ形式（16bit ステレオ WAV）に変換して保存する
pub fn import_audio(from: &Path, to: &Path) -> Result<(u32, u64), String> {
    let rate = super::decode::sample_rate(from)?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(to, spec).map_err(write_error)?;
    let mut frames = 0u64;
    super::decode::decode_file(from, &mut |chunk| {
        for [l, r] in chunk {
            writer.write_sample(to_i16(*l)).map_err(write_error)?;
            writer.write_sample(to_i16(*r)).map_err(write_error)?;
        }
        frames += chunk.len() as u64;
        Ok(())
    })?;
    writer.finalize().map_err(write_error)?;
    if frames == 0 {
        let _ = std::fs::remove_file(to);
        return Err("このファイルには音が入っていませんでした。".into());
    }
    Ok((rate, frames))
}

/// パソコンにある曲を、CD用の WAV（44.1kHz・588フレーム単位）にする
pub fn render_file_for_cd(
    from: &Path,
    to: &Path,
    expected_secs: f64,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let rate = super::decode::sample_rate(from)?;
    let mut resampler = Resampler::new(rate, CD_RATE);
    let mut sink: Box<dyn Sink> = Box::new(WavSink::create(to, true)?);
    let mut output = Vec::new();
    let mut done = 0u64;
    super::decode::decode_file(from, &mut |chunk| {
        output.clear();
        resampler.process(chunk, &mut output);
        sink.write(&output)?;
        done += chunk.len() as u64;
        on_progress((done as f64 / rate as f64 / expected_secs.max(1.0)).min(1.0));
        Ok(())
    })?;
    output.clear();
    resampler.finish(&mut output);
    sink.write(&output)?;
    sink.finish()
}

/// ファイル名に使えない文字を取り除く
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    trimmed.chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rec-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write_source(path: &Path, rate: u32, seconds: f64) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for n in 0..(rate as f64 * seconds) as u32 {
            let v = ((n as f32 * 0.05).sin() * 10000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn cut_resample_and_pad_for_cd() {
        let src = temp("src48.wav");
        write_source(&src, 48_000, 10.0);
        let out = temp("track.wav");
        let source = TrackSource {
            path: src,
            start_frame: 48_000,
            end_frame: 48_000 * 6,
        };
        let sink = WavSink::create(&out, true).unwrap();
        render_track(&source, vec![Box::new(sink)], &mut |_| {}).unwrap();
        let reader = hound::WavReader::open(&out).unwrap();
        assert_eq!(reader.spec().sample_rate, 44_100);
        let frames = reader.duration() as u64;
        assert_eq!(frames % CD_SECTOR_FRAMES, 0);
        assert!((frames as i64 - 44_100 * 5).abs() < 600);
    }

    #[test]
    fn short_track_is_padded_to_four_seconds() {
        let src = temp("src44.wav");
        write_source(&src, 44_100, 3.0);
        let out = temp("short.wav");
        let source = TrackSource {
            path: src,
            start_frame: 0,
            end_frame: 44_100 * 2,
        };
        render_track(
            &source,
            vec![Box::new(WavSink::create(&out, true).unwrap())],
            &mut |_| {},
        )
        .unwrap();
        let frames = hound::WavReader::open(&out).unwrap().duration() as u64;
        assert!(frames >= 4 * 44_100);
    }

    #[test]
    fn writes_mp3() {
        let src = temp("src-mp3.wav");
        write_source(&src, 44_100, 2.0);
        let out = temp("track.mp3");
        let source = TrackSource {
            path: src,
            start_frame: 0,
            end_frame: 44_100 * 2,
        };
        render_track(
            &source,
            vec![Box::new(Mp3Sink::create(&out, 192).unwrap())],
            &mut |_| {},
        )
        .unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.len() > 10_000);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("A/B: 恋の歌?"), "A_B_ 恋の歌_");
        assert_eq!(safe_file_name("  名前.  "), "名前");
    }
}
