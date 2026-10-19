//! 録音の解析：波形表示用のデータと、曲間（無音）の自動検出。

use std::path::Path;

use super::{BLOCK_SECS, db, rms_db};

/// 曲の最短の長さ（秒）。これより短い曲はできないようにする。
pub const MIN_TRACK_SECS: f64 = 30.0;

#[derive(Debug, Clone)]
pub struct SideAnalysis {
    pub sample_rate: u32,
    pub frames: u64,
    /// BLOCK_SECS ごとの RMS（dB）
    pub block_db: Vec<f32>,
    /// BLOCK_SECS ごとのピーク（-60dB〜0dB を 0〜255 にしたもの）。波形表示用。
    pub peaks: Vec<u8>,
}

impl SideAnalysis {
    pub fn duration(&self) -> f64 {
        self.frames as f64 / self.sample_rate as f64
    }

    #[cfg(test)]
    pub fn from_frames(sample_rate: u32, frames: &[[f32; 2]]) -> Self {
        let block_frames = ((sample_rate as f64 * BLOCK_SECS).round() as usize).max(1);
        let mut block_db = Vec::with_capacity(frames.len() / block_frames + 1);
        let mut peaks = Vec::with_capacity(block_db.capacity());
        for block in frames.chunks(block_frames) {
            block_db.push(rms_db(block));
            peaks.push(peak_byte(block));
        }
        Self {
            sample_rate,
            frames: frames.len() as u64,
            block_db,
            peaks,
        }
    }
}

fn peak_byte(block: &[[f32; 2]]) -> u8 {
    let peak = block
        .iter()
        .map(|[l, r]| l.abs().max(r.abs()))
        .fold(0f32, f32::max);
    (((db(peak) + 60.0) / 60.0).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// 録音した WAV（16bit ステレオ）を読み込んで解析する。メモリを使いすぎないよう少しずつ読む。
pub fn analyze_wav(path: &Path) -> Result<SideAnalysis, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("録音ファイルを開けませんでした（{e}）"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let block_frames = ((spec.sample_rate as f64 * BLOCK_SECS).round() as usize).max(1);
    let mut block_db = Vec::new();
    let mut peaks = Vec::new();
    let mut block: Vec<[f32; 2]> = Vec::with_capacity(block_frames);
    let mut frame = [0f32; 2];
    let mut ch = 0;
    let mut frames = 0u64;
    let mut push_sample = |v: f32, block: &mut Vec<[f32; 2]>| {
        if ch < 2 {
            frame[ch] = v;
        }
        ch += 1;
        if ch == channels {
            if channels == 1 {
                frame[1] = frame[0];
            }
            block.push(frame);
            ch = 0;
            true
        } else {
            false
        }
    };
    let mut flush = |block: &mut Vec<[f32; 2]>, force: bool| {
        if block.len() == block_frames || (force && !block.is_empty()) {
            block_db.push(rms_db(block));
            peaks.push(peak_byte(block));
            block.clear();
        }
    };
    match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            for s in reader.samples::<i32>() {
                let s = s.map_err(|e| format!("録音ファイルが壊れています（{e}）"))?;
                if push_sample(s as f32 * scale, &mut block) {
                    frames += 1;
                    flush(&mut block, false);
                }
            }
        }
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                let s = s.map_err(|e| format!("録音ファイルが壊れています（{e}）"))?;
                if push_sample(s, &mut block) {
                    frames += 1;
                    flush(&mut block, false);
                }
            }
        }
    }
    flush(&mut block, true);
    Ok(SideAnalysis {
        sample_rate: spec.sample_rate,
        frames,
        block_db,
        peaks,
    })
}

/// 曲分けの感度
#[derive(Debug, Clone, Copy)]
pub struct Sensitivity {
    /// ノイズ（針音）の大きさ＋何 dB までを「無音」とするか
    margin_db: f32,
    /// 何秒以上の無音を曲間とするか
    min_gap_secs: f64,
}

impl Sensitivity {
    pub fn from_name(name: &str) -> Self {
        match name {
            // 弱い：はっきりした曲間だけで分ける（分かれすぎを防ぐ）
            "low" => Self {
                margin_db: 4.0,
                min_gap_secs: 2.5,
            },
            // 強い：短い・浅い曲間でも分ける
            "high" => Self {
                margin_db: 9.0,
                min_gap_secs: 1.0,
            },
            _ => Self {
                margin_db: 6.0,
                min_gap_secs: 1.5,
            },
        }
    }
}

/// 片面の中で曲の区切り位置（秒）を見つける。面の最初と最後は含まない。
pub fn find_splits(side: &SideAnalysis, sensitivity: Sensitivity) -> Vec<f64> {
    let blocks = &side.block_db;
    if blocks.len() < 10 {
        return Vec::new();
    }
    // 針音の大きさ：下から 1% の音量（曲間・面の前後の無音はふつう全体の数%ある）
    let mut sorted: Vec<f32> = blocks.iter().copied().filter(|v| *v > -120.0).collect();
    if sorted.is_empty() {
        return Vec::new();
    }
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 100];
    let threshold = (floor + sensitivity.margin_db).clamp(-70.0, -35.0);

    // 無音が続く区間を探す。無音の中の一瞬の「プチッ」（2ブロックまで）は無視する。
    const CLICK_TOLERANCE: usize = 2;
    let min_gap_blocks = (sensitivity.min_gap_secs / BLOCK_SECS).round() as usize;
    let mut gaps: Vec<(usize, usize)> = Vec::new(); // (開始, 終了) ブロック
    let mut gap_start: Option<usize> = None;
    let mut last_quiet = 0usize;
    for (i, &v) in blocks.iter().enumerate() {
        if v < threshold {
            if gap_start.is_none() {
                gap_start = Some(i);
            }
            last_quiet = i;
        } else if let Some(start) = gap_start
            && i - last_quiet > CLICK_TOLERANCE
        {
            if last_quiet + 1 - start >= min_gap_blocks {
                gaps.push((start, last_quiet + 1));
            }
            gap_start = None;
        }
    }
    // 面の最初と最後の無音は曲間ではない
    let duration = side.duration();
    let mut candidates: Vec<(f64, f64)> = gaps
        .into_iter()
        .filter(|(s, e)| *s > 0 && *e < blocks.len())
        .map(|(s, e)| {
            let middle = (s + e) as f64 / 2.0 * BLOCK_SECS;
            let length = (e - s) as f64 * BLOCK_SECS;
            (middle, length)
        })
        .collect();
    // 長い曲間ほど確かなので、長いものから順に採用する
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut chosen: Vec<f64> = Vec::new();
    for (pos, _) in candidates {
        let far_from_edges = pos >= MIN_TRACK_SECS && duration - pos >= MIN_TRACK_SECS;
        let far_from_others = chosen.iter().all(|c| (c - pos).abs() >= MIN_TRACK_SECS);
        if far_from_edges && far_from_others {
            chosen.push(pos);
        }
    }
    chosen.sort_by(f64::total_cmp);
    chosen
}

/// 面の中で実際に音楽が始まる位置と終わる位置（秒）。
/// 針を落としたときの「ボン」という一瞬の音や、面の前後の無音を除くために使う。
/// 1秒のうち半分以上に音がある所を「音楽が鳴っている」とみなす。
pub fn sound_range(side: &SideAnalysis) -> (f64, f64) {
    const WINDOW: usize = 20;
    let threshold = super::recorder::SOUND_THRESHOLD_DB;
    let loud: Vec<bool> = side.block_db.iter().map(|v| *v > threshold).collect();
    let sustained = |from: usize| {
        loud[from..(from + WINDOW).min(loud.len())]
            .iter()
            .filter(|l| **l)
            .count()
            >= WINDOW / 2
    };
    let first = (0..loud.len()).find(|&i| loud[i] && sustained(i));
    let last = (0..loud.len())
        .rev()
        .find(|&i| loud[i] && sustained(i.saturating_sub(WINDOW - 1)));
    match (first, last) {
        (Some(f), Some(l)) if l > f => {
            let start = (f as f64 * BLOCK_SECS - 0.5).max(0.0);
            let end = ((l + 1) as f64 * BLOCK_SECS + 1.0).min(side.duration());
            (start, end)
        }
        _ => (0.0, side.duration()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (秒, 音の大きさ) の並びから、針音つきの合成録音を作る
    fn synth(parts: &[(f64, f32)]) -> SideAnalysis {
        let rate = 8_000;
        let mut frames = Vec::new();
        let mut seed = 12345u32;
        for &(secs, amp) in parts {
            for n in 0..(secs * rate as f64) as usize {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let noise = ((seed >> 16) as f32 / 32768.0 - 1.0) * 0.001; // 針音 約 -65dB
                let tone = amp * (n as f32 * 0.3).sin();
                frames.push([tone + noise, tone + noise]);
            }
        }
        SideAnalysis::from_frames(rate, &frames)
    }

    #[test]
    fn finds_gaps_between_songs() {
        let side = synth(&[
            (2.0, 0.0),
            (120.0, 0.3),
            (3.0, 0.0),
            (95.0, 0.2),
            (2.5, 0.0),
            (180.0, 0.25),
            (5.0, 0.0),
        ]);
        let splits = find_splits(&side, Sensitivity::from_name("normal"));
        assert_eq!(splits.len(), 2, "{splits:?}");
        assert!((splits[0] - 123.5).abs() < 0.3, "{splits:?}");
        assert!((splits[1] - 221.25).abs() < 0.3, "{splits:?}");
    }

    #[test]
    fn quiet_passage_inside_song_is_not_split_when_short() {
        // 1秒だけ静かになる部分は「ふつう」では分けない
        let side = synth(&[(60.0, 0.3), (1.0, 0.0), (60.0, 0.3)]);
        assert!(find_splits(&side, Sensitivity::from_name("normal")).is_empty());
    }

    #[test]
    fn no_track_shorter_than_minimum() {
        let side = synth(&[
            (40.0, 0.3),
            (2.0, 0.0),
            (10.0, 0.3),
            (2.0, 0.0),
            (40.0, 0.3),
        ]);
        let splits = find_splits(&side, Sensitivity::from_name("high"));
        let mut edges = vec![0.0];
        edges.extend(&splits);
        edges.push(side.duration());
        for pair in edges.windows(2) {
            assert!(pair[1] - pair[0] >= MIN_TRACK_SECS, "{splits:?}");
        }
    }

    #[test]
    fn clicks_inside_gap_are_ignored() {
        let side = synth(&[
            (60.0, 0.3),
            (1.0, 0.0),
            (0.05, 0.5),
            (1.0, 0.0),
            (60.0, 0.3),
        ]);
        let splits = find_splits(&side, Sensitivity::from_name("normal"));
        assert_eq!(splits.len(), 1, "{splits:?}");
    }

    #[test]
    fn sound_range_skips_needle_drop_thump() {
        // 針を落とした「ボン」→ 4秒の無音 → 曲 → 5秒の無音
        let side = synth(&[(1.0, 0.0), (0.1, 0.8), (4.0, 0.0), (60.0, 0.3), (5.0, 0.0)]);
        let (start, end) = sound_range(&side);
        assert!((start - 4.6).abs() < 0.2, "{start}");
        assert!((end - 66.1).abs() < 0.2, "{end}");
    }
}
