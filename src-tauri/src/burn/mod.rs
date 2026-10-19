//! 音楽CDの書き込み。OS ごとに標準の仕組み（または定番の道具）を使う。
//!   - Windows: IMAPI2（Windows 標準）
//!   - macOS:   drutil（macOS 標準）
//!   - Linux:   cdrskin（無ければ wodim / cdrecord）

use serde::Serialize;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;

/// CD-R に入る最大の長さ（秒）。79分57秒。
pub const CD_MAX_SECS: f64 = 79.0 * 60.0 + 57.0;
/// 曲と曲のあいだの無音（秒）
pub const CD_GAP_SECS: f64 = 2.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
#[allow(dead_code)] // OS によっては使わない状態がある
pub enum DiscStatus {
    /// 書き込みに必要な道具が入っていない（Linux のみ）
    NoTool,
    /// CDドライブが見つからない
    NoDrive,
    /// CDが入っていない
    NoDisc,
    /// CDが入っているが、何か書かれている
    NotBlank,
    /// CD ではない（DVD など）
    WrongMedia,
    /// 書き込める
    Ready,
    /// 状態がわからない（説明つき）
    Unknown { message: String },
}

pub fn check_disc() -> DiscStatus {
    platform::check_disc()
}

/// `tracks` は 44.1kHz/16bit/ステレオの WAV（588フレーム単位）。曲順どおりに並べる。
/// `on_progress` には 0.0〜1.0 を渡す。終わったら CD を取り出す。
pub fn burn(tracks: &[PathBuf], on_progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    platform::burn(tracks, on_progress)
}

/// "Track 03: 12 of 45 MB written" のような行から全体の進み具合を計算する
#[allow(dead_code)]
pub(crate) fn parse_track_progress(line: &str, total_tracks: usize) -> Option<f64> {
    let rest = line.trim_start().strip_prefix("Track")?;
    let (num, rest) = rest.trim_start().split_once(':')?;
    let track: usize = num.trim().parse().ok()?;
    let mut words = rest.split_whitespace();
    let done: f64 = words.next()?.parse().ok()?;
    if words.next()? != "of" {
        return None;
    }
    let size: f64 = words.next()?.parse().ok()?;
    if size <= 0.0 || track == 0 || total_tracks == 0 {
        return None;
    }
    let within = (done / size).clamp(0.0, 1.0);
    Some((((track - 1) as f64 + within) / total_tracks as f64).clamp(0.0, 1.0))
}

/// 行の中の「45%」のような数字を探す
#[allow(dead_code)]
pub(crate) fn parse_percent(line: &str) -> Option<f64> {
    let idx = line.find('%')?;
    let digits: String = line[..idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let value: f64 = digits.parse().ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_progress() {
        let p = parse_track_progress(
            "Track 02:   20 of   40 MB written (fifo 100%) [buf  99%]",
            4,
        )
        .unwrap();
        assert!((p - 0.375).abs() < 1e-9);
        assert!(parse_track_progress("Starting to write CD/DVD", 4).is_none());
    }

    #[test]
    fn percent() {
        assert_eq!(parse_percent("Burning: 45% done"), Some(0.45));
        assert_eq!(parse_percent("no number"), None);
    }
}
