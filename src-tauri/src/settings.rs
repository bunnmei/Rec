use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 利用者が設定画面で変えられる項目。
/// 項目は少なく保ち、すべて「わかりやすい選択肢」で選べるようにする。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// 録音に使う機器の ID。None なら自動で選ぶ（Sony / USB Audio CODEC を優先）
    pub device_id: Option<String>,
    /// "lp" = LP(片面30分) / "ep" = EP・シングル(片面10分) / "custom"
    pub record_type: String,
    /// record_type が "custom" のときの最大録音時間（分）
    pub custom_max_minutes: u32,
    /// 音が途切れてから自動で止めるまでの秒数
    pub silence_seconds: u32,
    /// 曲分けの感度 "low" / "normal" / "high"
    pub split_sensitivity: String,
    /// WAV に加えて MP3 も保存する
    pub save_mp3: bool,
    /// MP3 の音質（kbps）
    pub mp3_bitrate: u32,
    /// 保存先フォルダ。None なら「ミュージック/レコード録音」
    pub save_dir: Option<String>,
    /// "large" / "xlarge"
    pub font_size: String,
    /// "auto" / "light" / "dark"
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device_id: None,
            record_type: "lp".into(),
            custom_max_minutes: 30,
            silence_seconds: 15,
            split_sensitivity: "normal".into(),
            save_mp3: true,
            mp3_bitrate: 192,
            save_dir: None,
            font_size: "large".into(),
            theme: "auto".into(),
        }
    }
}

impl Settings {
    /// 片面あたりの最大録音時間（秒）
    pub fn max_seconds(&self) -> u64 {
        let minutes = match self.record_type.as_str() {
            "ep" => 10,
            "custom" => self.custom_max_minutes.clamp(1, 120),
            _ => 30,
        };
        minutes as u64 * 60
    }

    pub fn save_dir(&self) -> PathBuf {
        match &self.save_dir {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::audio_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("レコード録音"),
        }
    }

    pub fn load(path: &PathBuf) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &PathBuf) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}
