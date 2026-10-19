//! CD 用（44.1kHz）への変換。整数比のポリフェーズ・ウィンドウ付き sinc で行う。

/// 片側のタップ数。大きいほど高音質だが遅い。
const HALF_TAPS: usize = 32;

pub struct Resampler {
    up: usize,
    down: usize,
    /// phase * (2 * HALF_TAPS) + k
    table: Vec<f32>,
    /// まだ使い切っていない入力（先頭は absolute index = base）
    buffer: Vec<[f32; 2]>,
    base: i64,
    /// 次に出力するサンプルの番号
    next_out: u64,
    passthrough: bool,
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        let g = gcd(from_rate as usize, to_rate as usize);
        let up = to_rate as usize / g;
        let down = from_rate as usize / g;
        let passthrough = from_rate == to_rate;
        let mut table = Vec::new();
        if !passthrough {
            // 折り返しを防ぐため、低い方のナイキストより少し下で切る
            let cutoff = (up as f64 / down as f64).min(1.0) * 0.96;
            let taps = 2 * HALF_TAPS;
            table.reserve(up * taps);
            for phase in 0..up {
                let frac = phase as f64 / up as f64;
                let mut row: Vec<f64> = (0..taps)
                    .map(|k| {
                        // 入力 i + k - (HALF_TAPS - 1) と出力位置 i + frac の距離
                        let t = k as f64 - (HALF_TAPS as f64 - 1.0) - frac;
                        let x = std::f64::consts::PI * cutoff * t;
                        let sinc = if x.abs() < 1e-12 { 1.0 } else { x.sin() / x };
                        // Blackman 窓
                        let w_pos = (t + HALF_TAPS as f64) / (2.0 * HALF_TAPS as f64);
                        let window = if (0.0..=1.0).contains(&w_pos) {
                            let a = 2.0 * std::f64::consts::PI * w_pos;
                            0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
                        } else {
                            0.0
                        };
                        sinc * window
                    })
                    .collect();
                let sum: f64 = row.iter().sum();
                for v in &mut row {
                    *v /= sum;
                }
                table.extend(row.into_iter().map(|v| v as f32));
            }
        }
        Self {
            up,
            down,
            table,
            // 先頭より前は無音とみなす
            buffer: vec![[0.0; 2]; HALF_TAPS - 1],
            base: -(HALF_TAPS as i64 - 1),
            next_out: 0,
            passthrough,
        }
    }

    /// 入力を渡し、出せるだけの出力を `out` に追加する
    pub fn process(&mut self, input: &[[f32; 2]], out: &mut Vec<[f32; 2]>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.buffer.extend_from_slice(input);
        self.drain(out);
    }

    /// 残りを出し切る
    pub fn finish(&mut self, out: &mut Vec<[f32; 2]>) {
        if self.passthrough {
            return;
        }
        let consumed_input = self.base + self.buffer.len() as i64;
        // 入力の長さに見合った出力数
        let total_out = (consumed_input.max(0) as u64 * self.up as u64).div_ceil(self.down as u64);
        self.buffer
            .extend(std::iter::repeat_n([0.0; 2], 2 * HALF_TAPS));
        self.drain(out);
        let extra = (self.next_out.saturating_sub(total_out)) as usize;
        out.truncate(out.len().saturating_sub(extra));
    }

    fn drain(&mut self, out: &mut Vec<[f32; 2]>) {
        let taps = 2 * HALF_TAPS;
        loop {
            let pos = self.next_out * self.down as u64;
            let i = (pos / self.up as u64) as i64;
            let phase = (pos % self.up as u64) as usize;
            let first = i - (HALF_TAPS as i64 - 1) - self.base;
            if first < 0 {
                // 起こらないはずだが念のため
                self.next_out += 1;
                continue;
            }
            let first = first as usize;
            if first + taps > self.buffer.len() {
                break;
            }
            let coeffs = &self.table[phase * taps..(phase + 1) * taps];
            let mut acc = [0f32; 2];
            for (c, s) in coeffs.iter().zip(&self.buffer[first..first + taps]) {
                acc[0] += c * s[0];
                acc[1] += c * s[1];
            }
            out.push(acc);
            self.next_out += 1;
        }
        // もう使わない入力を捨てる
        let pos = self.next_out * self.down as u64;
        let i = (pos / self.up as u64) as i64;
        let keep_from = (i - (HALF_TAPS as i64 - 1) - self.base).max(0) as usize;
        let keep_from = keep_from.min(self.buffer.len());
        self.buffer.drain(..keep_from);
        self.base += keep_from as i64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|n| {
                let v = (2.0 * std::f32::consts::PI * freq * n as f32 / rate as f32).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn length_matches_ratio() {
        let input = sine(48_000, 1000.0, 48_000);
        let mut r = Resampler::new(48_000, 44_100);
        let mut out = Vec::new();
        for chunk in input.chunks(4096) {
            r.process(chunk, &mut out);
        }
        r.finish(&mut out);
        assert_eq!(out.len(), 44_100);
    }

    #[test]
    fn sine_survives_conversion() {
        let input = sine(48_000, 1000.0, 48_000);
        let mut r = Resampler::new(48_000, 44_100);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        r.finish(&mut out);
        let expected = sine(44_100, 1000.0, out.len());
        // 端を除いて元の正弦波とほぼ一致する
        let max_err = out[200..out.len() - 200]
            .iter()
            .zip(&expected[200..])
            .map(|(a, b)| (a[0] - b[0]).abs())
            .fold(0f32, f32::max);
        assert!(max_err < 0.01, "誤差 {max_err}");
    }

    #[test]
    fn passthrough_for_cd_rate() {
        let input = sine(44_100, 440.0, 1000);
        let mut r = Resampler::new(44_100, 44_100);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        r.finish(&mut out);
        assert_eq!(out, input);
    }
}
