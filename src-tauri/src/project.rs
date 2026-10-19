//! 作業中のレコード（録音した面・区切り位置・曲名）。
//! アプリを閉じても続きから始められるよう、変更のたびにファイルへ保存する。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Side {
    /// "A" / "B"
    pub label: String,
    /// 作業フォルダの中のファイル名
    pub file: String,
    pub sample_rate: u32,
    pub frames: u64,
}

impl Side {
    pub fn duration(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.frames as f64 / self.sample_rate as f64
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Project {
    pub sides: Vec<Side>,
    /// 曲の区切り位置（アルバム全体＝A面の後ろにB面をつなげた時間で、秒）
    pub splits: Vec<f64>,
    pub titles: Vec<String>,
    pub album_title: String,
    /// 画面のどの段階か "record" / "split" / "output"
    pub step: String,
    /// 録音ではなく、パソコンのファイルを読み込んだもの
    pub imported: bool,
}

impl Project {
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("project.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Project>(&text).ok())
            // 録音ファイルが消えている面は無かったことにする
            .map(|mut p| {
                p.sides.retain(|s| dir.join(&s.file).is_file());
                p
            })
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("project.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, dir.join("project.json"))
    }

    /// 各面がアルバム全体のどこから始まるか（秒）
    pub fn side_offsets(&self) -> Vec<f64> {
        let mut offset = 0.0;
        self.sides
            .iter()
            .map(|s| {
                let o = offset;
                offset += s.duration();
                o
            })
            .collect()
    }

    pub fn side_path(&self, dir: &Path, index: usize) -> PathBuf {
        dir.join(&self.sides[index].file)
    }

    /// 面を追加（同じ名前の面があれば置き換え）。A面→B面の順に並べる。
    pub fn put_side(&mut self, side: Side) {
        self.sides.retain(|s| s.label != side.label);
        self.sides.push(side);
        self.sides.sort_by(|a, b| a.label.cmp(&b.label));
    }
}

/// 面の中で、音が鳴っている範囲（面の中の秒）
#[derive(Debug, Clone, Copy)]
pub struct SideRange {
    pub offset: f64,
    pub sound_start: f64,
    pub sound_end: f64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub side: usize,
    /// アルバム全体での開始・終了（秒）
    pub start: f64,
    pub end: f64,
}

/// 区切り位置から曲の一覧を作る。面の境目は必ず曲の区切りになる。
pub fn compute_tracks(sides: &[SideRange], splits: &[f64]) -> Vec<Track> {
    let mut tracks = Vec::new();
    for (index, side) in sides.iter().enumerate() {
        let start = side.offset + side.sound_start;
        let end = side.offset + side.sound_end;
        if end <= start {
            continue;
        }
        let mut cuts = vec![start];
        let mut inner: Vec<f64> = splits
            .iter()
            .copied()
            .filter(|s| *s > start + 0.5 && *s < end - 0.5)
            .collect();
        inner.sort_by(f64::total_cmp);
        cuts.extend(inner);
        cuts.push(end);
        for pair in cuts.windows(2) {
            tracks.push(Track {
                side: index,
                start: pair[0],
                end: pair[1],
            });
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_respect_side_boundaries() {
        let sides = [
            SideRange {
                offset: 0.0,
                sound_start: 1.0,
                sound_end: 600.0,
            },
            SideRange {
                offset: 610.0,
                sound_start: 0.5,
                sound_end: 500.0,
            },
        ];
        let tracks = compute_tracks(&sides, &[300.0, 900.0, 605.0]);
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[0].start, 1.0);
        assert_eq!(tracks[1].end, 600.0);
        assert_eq!(tracks[2].start, 610.5);
        assert_eq!(tracks[2].side, 1);
        assert_eq!(tracks[3].end, 1110.0);
    }
}
