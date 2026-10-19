pub mod analysis;
pub mod decode;
pub mod devices;
pub mod export;
pub mod recorder;
pub mod resample;

/// 音量を調べる単位（秒）。録音の自動停止・曲分け・波形表示はすべてこの単位で行う。
pub const BLOCK_SECS: f64 = 0.05;

/// 振幅（0〜1）を dB に
pub fn db(amplitude: f32) -> f32 {
    20.0 * amplitude.max(1e-9).log10()
}

/// ステレオのブロックの RMS を dB で
pub fn rms_db(block: &[[f32; 2]]) -> f32 {
    if block.is_empty() {
        return db(0.0);
    }
    let sum: f64 = block
        .iter()
        .map(|[l, r]| (*l as f64 * *l as f64 + *r as f64 * *r as f64) / 2.0)
        .sum();
    db((sum / block.len() as f64).sqrt() as f32)
}
