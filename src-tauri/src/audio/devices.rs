use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// レコードプレーヤーらしい機器なら true
    pub recommended: bool,
}

/// レコードプレーヤー（USB オーディオ）らしい名前かどうか
fn looks_like_turntable(name: &str) -> bool {
    let lower = name.to_lowercase();
    [
        "usb audio codec",
        "sony",
        "ps-lx",
        "turntable",
        "phono",
        "usb audio",
    ]
    .iter()
    .any(|key| lower.contains(key))
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| device.to_string())
}

/// Linux（ALSA）では同じ機器が「hw:」「plughw:」などで何度も出てくるので、
/// 一番つながりやすいものだけを残す。小さい数ほど優先。
fn id_rank(id: &str) -> u8 {
    let kind = id.split_once(':').map_or(id, |(_, rest)| rest);
    if kind.starts_with("sysdefault:") {
        0
    } else if kind.starts_with("plughw:") {
        1
    } else if kind.starts_with("front:") {
        2
    } else if kind.starts_with("hw:") {
        3
    } else {
        4
    }
}

fn is_useless(name: &str) -> bool {
    name.starts_with("Discard all samples")
}

/// 入力機器を、おすすめ順・重複なしで返す
fn input_devices() -> Vec<(cpal::Device, DeviceInfo)> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    let mut list: Vec<(cpal::Device, DeviceInfo)> = devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let name = device_name(&device);
            if is_useless(&name) {
                return None;
            }
            let info = DeviceInfo {
                recommended: looks_like_turntable(&name),
                id,
                name,
            };
            Some((device, info))
        })
        .collect();
    list.sort_by_key(|(_, d)| (!d.recommended, id_rank(&d.id)));
    let mut seen = std::collections::HashSet::new();
    list.retain(|(_, d)| seen.insert(d.name.clone()));
    list
}

pub fn list_input_devices() -> Vec<DeviceInfo> {
    input_devices().into_iter().map(|(_, info)| info).collect()
}

/// 設定で選ばれた機器 → レコードプレーヤーらしい機器 → 既定の入力機器 の順に探す
pub fn find_input_device(preferred_id: Option<&str>) -> Option<(cpal::Device, String)> {
    let host = cpal::default_host();
    if let Some(id) = preferred_id
        && let Ok(parsed) = id.parse()
        && let Some(device) = host.device_by_id(&parsed)
    {
        let name = device_name(&device);
        return Some((device, name));
    }
    if let Some((device, info)) = input_devices().into_iter().find(|(_, d)| d.recommended) {
        return Some((device, info.name));
    }
    let device = host.default_input_device()?;
    let name = device_name(&device);
    Some((device, name))
}

#[cfg(test)]
mod tests {
    use super::looks_like_turntable;

    #[test]
    fn detects_sony_usb_codec() {
        assert!(looks_like_turntable("USB AUDIO  CODEC"));
        assert!(looks_like_turntable("Sony PS-LX310BT"));
        assert!(looks_like_turntable("マイク (USB Audio CODEC )"));
        assert!(!looks_like_turntable("Built-in Microphone"));
    }

    /// 実際の機器一覧を表示する（`cargo test -- --ignored --nocapture prints_devices`）
    #[test]
    #[ignore]
    fn prints_devices() {
        for d in super::list_input_devices() {
            println!(
                "{} {:?} [{}]",
                if d.recommended { "★" } else { " " },
                d.name,
                d.id
            );
        }
        println!(
            "選ばれる機器: {:?}",
            super::find_input_device(None).map(|(_, n)| n)
        );
    }

    #[test]
    fn alsa_duplicates_prefer_sysdefault() {
        assert!(super::id_rank("alsa:sysdefault:CARD=X") < super::id_rank("alsa:hw:CARD=2,DEV=0"));
        assert!(
            super::id_rank("alsa:plughw:CARD=2,DEV=0") < super::id_rank("alsa:hw:CARD=2,DEV=0")
        );
    }
}
