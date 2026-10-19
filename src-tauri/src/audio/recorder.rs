//! レコードの録音。
//!
//! 録音を始めても、針が落ちて音が聞こえるまでは保存しない（先頭の無音を自動で詰める）。
//! 音が聞こえてからは、音が一定時間途切れたとき（レコードの終わり）・片面の最大時間に
//! 達したとき・「止める」が押されたときに止まる。
//!
//! 終わりの溝のプチッという周期的な音で止まらなくならないよう、
//! 「1秒間のうち音がある時間の割合」で判断する。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, SupportedStreamConfig};
use serde::Serialize;

use super::devices::find_input_device;
use super::{BLOCK_SECS, rms_db};

/// 音があると判断する最小の音量（dBFS, RMS）。レコードの針音（-55〜-65dB程度）より上。
pub const SOUND_THRESHOLD_DB: f32 = -48.0;
/// 1秒（20ブロック）のうち、何ブロック以上音があれば「鳴っている」とするか
const LOUD_BLOCKS_PER_WINDOW: usize = 6;
const WINDOW_BLOCKS: usize = 20;
/// 録音開始と判断するために連続して必要な「音あり」ブロック数
const START_BLOCKS: usize = 3;
/// 音が聞こえる前の部分も少しだけ残す（ブロック数 = 1秒）
const PREROLL_BLOCKS: usize = 20;
/// 自動停止したとき、最後に残す無音（ブロック数 = 1秒）
const TAIL_BLOCKS: usize = 20;

#[derive(Debug, Clone)]
pub struct RecorderConfig {
    pub device_id: Option<String>,
    pub path: PathBuf,
    pub max_seconds: u64,
    pub silence_seconds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecState {
    /// 針が落ちて音が聞こえるのを待っている
    Waiting,
    /// 録音中（音が鳴っている）
    Recording,
    /// 録音中だが、音が途切れている（このまま続けば自動で止まる）
    Quiet,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelInfo {
    pub state: RecState,
    /// 0.0〜1.0
    pub peak_left: f32,
    pub peak_right: f32,
    /// 録音できた長さ（秒）
    pub elapsed: f64,
    /// 音が途切れている時間（秒）
    pub quiet_seconds: f64,
    pub silence_seconds: u32,
    pub max_seconds: u64,
    pub waiting_seconds: f64,
    pub clipping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    Silence,
    MaxTime,
    Manual,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub frames: u64,
    pub reason: StopReason,
    pub error: Option<String>,
}

pub struct RecorderHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl RecorderHandle {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// 録音を始める。音声機器の準備に失敗したらすぐにエラーを返す。
pub fn start(
    config: RecorderConfig,
    on_level: impl Fn(LevelInfo) + Send + 'static,
    on_finish: impl FnOnce(RecordingResult) + Send + 'static,
) -> Result<RecorderHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let stop_flag = stop.clone();
    let thread = std::thread::Builder::new()
        .name("recorder".into())
        .spawn(move || {
            // cpal の Stream は Send でない環境があるため、このスレッドの中で作る
            let (device, _name) = match find_input_device(config.device_id.as_deref()) {
                Some(found) => found,
                None => {
                    let _ = ready_tx.send(Err(NO_DEVICE.into()));
                    return;
                }
            };
            let stream_config = match choose_config(&device) {
                Ok(c) => c,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let (data_tx, data_rx) = mpsc::channel::<Vec<f32>>();
            let (err_tx, err_rx) = mpsc::channel::<String>();
            let stream = match build_stream(&device, &stream_config, data_tx, err_tx) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!("{OPEN_FAILED}（{e}）")));
                return;
            }
            let _ = ready_tx.send(Ok(()));

            let result = run_worker(
                &config,
                stream_config.sample_rate(),
                stream_config.channels() as usize,
                data_rx,
                err_rx,
                &stop_flag,
                &on_level,
            );
            drop(stream);
            on_finish(result);
        })
        .map_err(|e| e.to_string())?;

    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(RecorderHandle {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err(OPEN_FAILED.into())
        }
    }
}

const NO_DEVICE: &str = "レコードプレーヤーが見つかりません。USBケーブルがつながっているか、プレーヤーの電源が入っているか確かめてください。";
const OPEN_FAILED: &str = "レコードプレーヤーの音を受け取れませんでした。USBケーブルを一度抜いて、もう一度差してください。";

/// CD と同じ 44.1kHz をできるだけ選ぶ。だめなら 48kHz、それもだめなら機器の既定。
fn choose_config(device: &cpal::Device) -> Result<SupportedStreamConfig, String> {
    let format_rank = |f: SampleFormat| match f {
        SampleFormat::I16 => 3,
        SampleFormat::F32 => 2,
        SampleFormat::I32 => 1,
        _ => 0,
    };
    if let Ok(configs) = device.supported_input_configs() {
        let configs: Vec<_> = configs
            .filter(|c| format_rank(c.sample_format()) > 0)
            .collect();
        for rate in [44_100, 48_000] {
            for channels in [2, 1] {
                if let Some(found) = configs
                    .iter()
                    .filter(|c| c.channels() == channels && c.contains_rate(rate))
                    .max_by_key(|c| format_rank(c.sample_format()))
                {
                    return Ok((*found).with_sample_rate(rate));
                }
            }
        }
    }
    device
        .default_input_config()
        .map_err(|e| format!("{OPEN_FAILED}（{e}）"))
}

fn build_stream(
    device: &cpal::Device,
    config: &SupportedStreamConfig,
    data_tx: mpsc::Sender<Vec<f32>>,
    err_tx: mpsc::Sender<String>,
) -> Result<cpal::Stream, String> {
    fn make<T>(
        device: &cpal::Device,
        config: &SupportedStreamConfig,
        data_tx: mpsc::Sender<Vec<f32>>,
        err_tx: mpsc::Sender<String>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        device.build_input_stream::<T, _, _>(
            config.config(),
            move |data: &[T], _| {
                let chunk: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();
                let _ = data_tx.send(chunk);
            },
            move |err| {
                let _ = err_tx.send(err.to_string());
            },
            None,
        )
    }
    let result = match config.sample_format() {
        SampleFormat::I16 => make::<i16>(device, config, data_tx, err_tx),
        SampleFormat::I32 => make::<i32>(device, config, data_tx, err_tx),
        SampleFormat::F32 => make::<f32>(device, config, data_tx, err_tx),
        SampleFormat::U16 => make::<u16>(device, config, data_tx, err_tx),
        other => return Err(format!("{OPEN_FAILED}（未対応の形式 {other}）")),
    };
    result.map_err(|e| format!("{OPEN_FAILED}（{e}）"))
}

type Block = Vec<[f32; 2]>;

/// 「1秒間のうち音がある割合」で鳴っているかを判断する
#[derive(Default)]
pub struct LoudnessGate {
    window: VecDeque<bool>,
    loud_count: usize,
}

impl LoudnessGate {
    /// ブロックの音量を入れ、直近1秒が「鳴っている」かを返す
    pub fn push(&mut self, block_db: f32) -> bool {
        let loud = block_db > SOUND_THRESHOLD_DB;
        self.window.push_back(loud);
        if loud {
            self.loud_count += 1;
        }
        if self.window.len() > WINDOW_BLOCKS && self.window.pop_front() == Some(true) {
            self.loud_count -= 1;
        }
        self.loud_count >= LOUD_BLOCKS_PER_WINDOW
    }
}

struct Worker<'a> {
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    written_blocks: u64,
    written_frames: u64,
    pending: Vec<Block>,
    preroll: VecDeque<Block>,
    state: RecState,
    gate: LoudnessGate,
    start_run: usize,
    waiting_blocks: u64,
    config: &'a RecorderConfig,
}

impl Worker<'_> {
    fn write(&mut self, block: &Block) {
        for frame in block {
            for sample in frame {
                let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
                let _ = self.writer.write_sample(value);
            }
        }
        self.written_frames += block.len() as u64;
        self.written_blocks += 1;
        // 万一パソコンが止まっても、それまでの録音が残るよう約5秒ごとに書き出す
        if self.written_blocks.is_multiple_of(100) {
            let _ = self.writer.flush();
        }
    }

    fn flush_pending(&mut self, max_blocks: usize) {
        let pending = std::mem::take(&mut self.pending);
        for block in pending.iter().take(max_blocks) {
            self.write(block);
        }
    }

    fn recorded_seconds(&self) -> f64 {
        (self.written_blocks + self.pending.len() as u64) as f64 * BLOCK_SECS
    }

    /// 1ブロック分を処理する。止めるべきなら理由を返す。
    fn process(&mut self, block: Block, block_db: f32) -> Option<StopReason> {
        let loud = self.gate.push(block_db);
        match self.state {
            RecState::Waiting => {
                self.waiting_blocks += 1;
                self.start_run = if block_db > SOUND_THRESHOLD_DB {
                    self.start_run + 1
                } else {
                    0
                };
                self.preroll.push_back(block);
                if self.preroll.len() > PREROLL_BLOCKS {
                    self.preroll.pop_front();
                }
                if self.start_run >= START_BLOCKS {
                    let preroll = std::mem::take(&mut self.preroll);
                    for b in &preroll {
                        self.write(b);
                    }
                    self.state = RecState::Recording;
                }
                None
            }
            RecState::Recording | RecState::Quiet => {
                if loud {
                    self.flush_pending(usize::MAX);
                    self.write(&block);
                    self.state = RecState::Recording;
                } else {
                    self.pending.push(block);
                    self.state = RecState::Quiet;
                    let quiet_secs = self.pending.len() as f64 * BLOCK_SECS;
                    if quiet_secs >= self.config.silence_seconds as f64 {
                        self.flush_pending(TAIL_BLOCKS);
                        return Some(StopReason::Silence);
                    }
                }
                if self.recorded_seconds() >= self.config.max_seconds as f64 {
                    self.flush_pending(usize::MAX);
                    return Some(StopReason::MaxTime);
                }
                None
            }
        }
    }
}

fn run_worker(
    config: &RecorderConfig,
    sample_rate: u32,
    channels: usize,
    data_rx: mpsc::Receiver<Vec<f32>>,
    err_rx: mpsc::Receiver<String>,
    stop: &AtomicBool,
    on_level: &dyn Fn(LevelInfo),
) -> RecordingResult {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut result = RecordingResult {
        path: config.path.clone(),
        sample_rate,
        frames: 0,
        reason: StopReason::Manual,
        error: None,
    };
    if let Some(parent) = config.path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let writer = match hound::WavWriter::create(&config.path, spec) {
        Ok(w) => w,
        Err(e) => {
            result.reason = StopReason::Error;
            result.error = Some(format!("録音ファイルを作れませんでした（{e}）"));
            return result;
        }
    };
    let mut worker = Worker {
        writer,
        written_blocks: 0,
        written_frames: 0,
        pending: Vec::new(),
        preroll: VecDeque::new(),
        state: RecState::Waiting,
        gate: LoudnessGate::default(),
        start_run: 0,
        waiting_blocks: 0,
        config,
    };
    let block_frames = ((sample_rate as f64 * BLOCK_SECS).round() as usize).max(1);
    let mut block: Block = Vec::with_capacity(block_frames);
    let mut leftover: Vec<f32> = Vec::new();
    let channels = channels.max(1);

    let reason = 'outer: loop {
        if stop.load(Ordering::SeqCst) {
            break StopReason::Manual;
        }
        if let Ok(err) = err_rx.try_recv() {
            // 機器が抜かれたなど。ここまでの録音は残す。
            result.error = Some(format!(
                "録音の途中でレコードプレーヤーとの接続が切れました。ここまでの録音は残っています。（{err}）"
            ));
            break StopReason::Error;
        }
        let chunk = match data_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => chunk,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break StopReason::Error,
        };
        leftover.extend_from_slice(&chunk);
        let usable = leftover.len() / channels * channels;
        for frame in leftover[..usable].chunks_exact(channels) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { frame[0] };
            block.push([left, right]);
            if block.len() == block_frames {
                let full = std::mem::replace(&mut block, Vec::with_capacity(block_frames));
                let (block_db, peak_left, peak_right) = measure(&full);
                if let Some(reason) = worker.process(full, block_db) {
                    break 'outer reason;
                }
                on_level(LevelInfo {
                    state: worker.state,
                    peak_left,
                    peak_right,
                    elapsed: worker.recorded_seconds(),
                    quiet_seconds: worker.pending.len() as f64 * BLOCK_SECS,
                    silence_seconds: config.silence_seconds,
                    max_seconds: config.max_seconds,
                    waiting_seconds: worker.waiting_blocks as f64 * BLOCK_SECS,
                    clipping: peak_left >= 0.99 || peak_right >= 0.99,
                });
            }
        }
        leftover.drain(..usable);
    };

    if matches!(reason, StopReason::Manual | StopReason::Error) {
        worker.flush_pending(TAIL_BLOCKS);
    }
    result.reason = reason;
    result.frames = worker.written_frames;
    if let Err(e) = worker.writer.finalize() {
        result.reason = StopReason::Error;
        result.error = Some(format!("録音ファイルを保存できませんでした（{e}）"));
    }
    result
}

fn measure(block: &Block) -> (f32, f32, f32) {
    let mut peak_left = 0f32;
    let mut peak_right = 0f32;
    for [l, r] in block {
        peak_left = peak_left.max(l.abs());
        peak_right = peak_right.max(r.abs());
    }
    (rms_db(block), peak_left, peak_right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_block(amplitude: f32) -> Block {
        (0..2205)
            .map(|i| {
                let v = amplitude * (i as f32 * 0.05).sin();
                [v, v]
            })
            .collect()
    }

    fn run(
        blocks: impl IntoIterator<Item = Block>,
        silence_seconds: u32,
        max_seconds: u64,
    ) -> (StopReason, u64) {
        let dir = std::env::temp_dir().join(format!("rec-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{silence_seconds}-{max_seconds}.wav"));
        let config = RecorderConfig {
            device_id: None,
            path: path.clone(),
            max_seconds,
            silence_seconds,
        };
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut worker = Worker {
            writer: hound::WavWriter::create(&path, spec).unwrap(),
            written_blocks: 0,
            written_frames: 0,
            pending: Vec::new(),
            preroll: VecDeque::new(),
            state: RecState::Waiting,
            gate: LoudnessGate::default(),
            start_run: 0,
            waiting_blocks: 0,
            config: &config,
        };
        let mut reason = StopReason::Manual;
        for b in blocks {
            let (d, _, _) = measure(&b);
            if let Some(r) = worker.process(b, d) {
                reason = r;
                break;
            }
        }
        (reason, worker.written_blocks)
    }

    #[test]
    fn waits_for_sound_then_stops_after_silence() {
        // 10秒の無音（針を落とす前）→ 20秒の音 → 30秒の無音
        let blocks = std::iter::repeat_n(tone_block(0.0), 200)
            .chain(std::iter::repeat_n(tone_block(0.3), 400))
            .chain(std::iter::repeat_n(tone_block(0.0), 600));
        let (reason, written) = run(blocks, 15, 1800);
        assert_eq!(reason, StopReason::Silence);
        // 先頭の無音はほぼ捨てられ、末尾は最大1秒＋判定の遅れ分だけ残る
        let secs = written as f64 * BLOCK_SECS;
        assert!(secs > 20.0 && secs < 24.0, "録音時間 {secs}");
    }

    #[test]
    fn runout_groove_clicks_do_not_keep_recording() {
        // 曲のあと、1.8秒ごとに一瞬だけ「プチッ」と鳴る（終わりの溝）
        let mut blocks: Vec<Block> = std::iter::repeat_n(tone_block(0.3), 200).collect();
        for i in 0..1200 {
            blocks.push(if i % 36 == 0 {
                tone_block(0.5)
            } else {
                tone_block(0.0005)
            });
        }
        let (reason, _) = run(blocks, 15, 1800);
        assert_eq!(reason, StopReason::Silence);
    }

    #[test]
    fn stops_at_max_time() {
        let blocks = std::iter::repeat_n(tone_block(0.3), 2000);
        let (reason, written) = run(blocks, 15, 60);
        assert_eq!(reason, StopReason::MaxTime);
        assert_eq!(written, 1200);
    }

    #[test]
    fn short_gap_between_songs_continues() {
        // 曲間の3秒の無音では止まらない
        let blocks = std::iter::repeat_n(tone_block(0.3), 200)
            .chain(std::iter::repeat_n(tone_block(0.0), 60))
            .chain(std::iter::repeat_n(tone_block(0.3), 200));
        let (reason, written) = run(blocks, 15, 1800);
        assert_eq!(reason, StopReason::Manual);
        assert!(written >= 440);
    }

    /// 実際の機器で録音を始めて、音量の知らせが届くかを確かめる
    /// （`cargo test -- --ignored --nocapture records_from_real_device`）
    #[test]
    #[ignore]
    fn records_from_real_device() {
        use std::sync::atomic::AtomicUsize;
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let (tx, rx) = mpsc::channel();
        let path = std::env::temp_dir().join("rec-real-device.wav");
        let handle = start(
            RecorderConfig {
                device_id: None,
                path,
                max_seconds: 60,
                silence_seconds: 15,
            },
            move |level| {
                if counter.fetch_add(1, Ordering::SeqCst).is_multiple_of(10) {
                    println!(
                        "{:?} L={:.3} R={:.3}",
                        level.state, level.peak_left, level.peak_right
                    );
                }
            },
            move |result| {
                let _ = tx.send(result);
            },
        )
        .expect("録音を始められること");
        std::thread::sleep(Duration::from_secs(2));
        handle.request_stop();
        let result = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        println!("{result:?}");
        assert!(count.load(Ordering::SeqCst) > 20, "音量の知らせが届くこと");
    }
}
