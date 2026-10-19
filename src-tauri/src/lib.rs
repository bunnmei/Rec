mod audio;
mod burn;
mod project;
mod settings;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use audio::analysis::{self, Sensitivity, SideAnalysis};
use audio::devices::{self, DeviceInfo};
use audio::export::{self, Mp3Sink, Sink, TrackSource, WavSink};
use audio::recorder::{self, RecorderConfig, RecorderHandle, RecordingResult, StopReason};
use burn::DiscStatus;
use project::{Project, Side, SideRange, Track};
use settings::Settings;

type CmdResult<T> = Result<T, String>;

struct AppState {
    settings_file: PathBuf,
    project_dir: PathBuf,
    settings: Mutex<Settings>,
    project: Mutex<Project>,
    /// 録音ファイル名 → 解析結果
    analyses: Mutex<HashMap<String, SideAnalysis>>,
    recorder: Mutex<Option<RecorderHandle>>,
    /// 保存・CD書き込み中
    busy: AtomicBool,
}

impl AppState {
    fn save_project(&self, project: &Project) {
        if let Err(e) = project.save(&self.project_dir) {
            eprintln!("作業データを保存できませんでした: {e}");
        }
    }

    fn analysis_for(&self, project: &Project, index: usize) -> CmdResult<SideAnalysis> {
        let file = project.sides[index].file.clone();
        if let Some(found) = self.analyses.lock().unwrap().get(&file) {
            return Ok(found.clone());
        }
        let result = analysis::analyze_wav(&project.side_path(&self.project_dir, index))?;
        self.analyses.lock().unwrap().insert(file, result.clone());
        Ok(result)
    }

    fn side_ranges(&self, project: &Project) -> CmdResult<Vec<SideRange>> {
        let offsets = project.side_offsets();
        (0..project.sides.len())
            .map(|i| {
                let a = self.analysis_for(project, i)?;
                let (sound_start, sound_end) = analysis::sound_range(&a);
                Ok(SideRange {
                    offset: offsets[i],
                    sound_start,
                    sound_end,
                })
            })
            .collect()
    }

    fn tracks(&self, project: &Project) -> CmdResult<Vec<Track>> {
        Ok(project::compute_tracks(
            &self.side_ranges(project)?,
            &project.splits,
        ))
    }

    /// 指定した面の区切りを自動で付け直す（他の面の区切りはそのまま）
    fn auto_split_side(&self, project: &mut Project, index: usize) -> CmdResult<()> {
        let sensitivity = Sensitivity::from_name(&self.settings.lock().unwrap().split_sensitivity);
        let offset = project.side_offsets()[index];
        let duration = project.sides[index].duration();
        let a = self.analysis_for(project, index)?;
        project
            .splits
            .retain(|s| *s <= offset || *s >= offset + duration);
        project.splits.extend(
            analysis::find_splits(&a, sensitivity)
                .into_iter()
                .map(|s| s + offset),
        );
        project.splits.sort_by(f64::total_cmp);
        Ok(())
    }
}

fn is_recording(state: &AppState) -> bool {
    state
        .recorder
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|h| !h.is_finished())
}

fn friendly_lock_error() -> String {
    "ほかの作業が終わるまでお待ちください。".into()
}

// ───────── 設定 ─────────

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<Settings> {
    settings
        .save(&state.settings_file)
        .map_err(|e| format!("設定を保存できませんでした（{e}）"))?;
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

#[tauri::command]
fn list_devices() -> Vec<DeviceInfo> {
    devices::list_input_devices()
}

#[tauri::command]
fn default_save_dir(state: State<AppState>) -> String {
    state
        .settings
        .lock()
        .unwrap()
        .save_dir()
        .to_string_lossy()
        .into_owned()
}

// ───────── アルバム（作業中のレコード） ─────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SideView {
    label: String,
    path: String,
    offset: f64,
    duration: f64,
    block_secs: f64,
    peaks: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AlbumView {
    sides: Vec<SideView>,
    tracks: Vec<Track>,
    splits: Vec<f64>,
    titles: Vec<String>,
    album_title: String,
    step: String,
    imported: bool,
    /// CD にしたときの長さ（曲間の無音を含む）
    cd_seconds: f64,
    cd_max_seconds: f64,
}

fn album_view(state: &AppState, project: &Project) -> CmdResult<AlbumView> {
    let tracks = state.tracks(project)?;
    let offsets = project.side_offsets();
    let mut sides = Vec::new();
    for (i, side) in project.sides.iter().enumerate() {
        let a = state.analysis_for(project, i)?;
        sides.push(SideView {
            label: side.label.clone(),
            path: project
                .side_path(&state.project_dir, i)
                .to_string_lossy()
                .into_owned(),
            offset: offsets[i],
            duration: side.duration(),
            block_secs: audio::BLOCK_SECS,
            peaks: a.peaks,
        });
    }
    let music: f64 = tracks.iter().map(|t| t.end - t.start).sum();
    let gaps = tracks.len().saturating_sub(1) as f64 * burn::CD_GAP_SECS;
    Ok(AlbumView {
        sides,
        cd_seconds: music + gaps,
        cd_max_seconds: burn::CD_MAX_SECS,
        tracks,
        splits: project.splits.clone(),
        titles: project.titles.clone(),
        album_title: project.album_title.clone(),
        step: project.step.clone(),
        imported: project.imported,
    })
}

fn default_album_title() -> String {
    chrono::Local::now()
        .format("レコード %Y年%m月%d日")
        .to_string()
}

/// 作業中のレコードを返す。何も録音されていなければ None。
#[tauri::command]
async fn get_album(state: State<'_, AppState>) -> CmdResult<Option<AlbumView>> {
    let project = state.project.lock().unwrap().clone();
    if project.sides.is_empty() {
        return Ok(None);
    }
    album_view(&state, &project).map(Some)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumUpdate {
    splits: Option<Vec<f64>>,
    titles: Option<Vec<String>>,
    album_title: Option<String>,
    step: Option<String>,
}

#[tauri::command]
async fn update_album(state: State<'_, AppState>, update: AlbumUpdate) -> CmdResult<AlbumView> {
    let project = {
        let mut project = state.project.lock().unwrap();
        if let Some(mut splits) = update.splits {
            splits.retain(|s| s.is_finite());
            splits.sort_by(f64::total_cmp);
            splits.dedup_by(|a, b| (*a - *b).abs() < 0.1);
            project.splits = splits;
        }
        if let Some(titles) = update.titles {
            project.titles = titles;
        }
        if let Some(title) = update.album_title {
            project.album_title = title;
        }
        if let Some(step) = update.step {
            project.step = step;
        }
        project.clone()
    };
    state.save_project(&project);
    album_view(&state, &project)
}

/// 全部の面の区切りを自動で付け直す
#[tauri::command]
async fn auto_split(state: State<'_, AppState>) -> CmdResult<AlbumView> {
    let mut project = state.project.lock().unwrap().clone();
    for i in 0..project.sides.len() {
        state.auto_split_side(&mut project, i)?;
    }
    project.titles.clear();
    *state.project.lock().unwrap() = project.clone();
    state.save_project(&project);
    album_view(&state, &project)
}

/// 作業中のレコードを消して、最初からにする
#[tauri::command]
fn new_album(state: State<AppState>) -> CmdResult<()> {
    if is_recording(&state) || state.busy.load(Ordering::SeqCst) {
        return Err(friendly_lock_error());
    }
    reset_project(&state);
    Ok(())
}

fn reset_project(state: &AppState) {
    let _ = std::fs::remove_dir_all(&state.project_dir);
    let _ = std::fs::create_dir_all(&state.project_dir);
    state.analyses.lock().unwrap().clear();
    *state.project.lock().unwrap() = Project {
        album_title: default_album_title(),
        step: "record".into(),
        ..Default::default()
    };
}

/// パソコンにある音声ファイルを読み込む（A面として扱う）
#[tauri::command]
async fn import_audio(state: State<'_, AppState>, path: String) -> CmdResult<AlbumView> {
    if is_recording(&state) || state.busy.load(Ordering::SeqCst) {
        return Err(friendly_lock_error());
    }
    reset_project(&state);
    let file = "side-a.wav".to_string();
    let target = state.project_dir.join(&file);
    let path_for_title = path.clone();
    let (sample_rate, frames) = tauri::async_runtime::spawn_blocking(move || {
        export::import_audio(Path::new(&path), &target)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut project = state.project.lock().unwrap().clone();
    project.put_side(Side {
        label: "A".into(),
        file,
        sample_rate,
        frames,
    });
    project.step = "split".into();
    project.imported = true;
    if let Some(stem) = Path::new(&path_for_title).file_stem() {
        project.album_title = stem.to_string_lossy().into_owned();
    }
    state.auto_split_side(&mut project, 0)?;
    *state.project.lock().unwrap() = project.clone();
    state.save_project(&project);
    album_view(&state, &project)
}

// ───────── 録音 ─────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RecordingFinished {
    side: String,
    reason: StopReason,
    seconds: f64,
    error: Option<String>,
}

#[tauri::command]
fn start_recording(app: AppHandle, state: State<AppState>, side: String) -> CmdResult<()> {
    let mut slot = state.recorder.lock().unwrap();
    if slot.as_ref().is_some_and(|h| !h.is_finished()) {
        return Err("いま録音中です。".into());
    }
    if state.busy.load(Ordering::SeqCst) {
        return Err(friendly_lock_error());
    }
    let side = if side == "B" { "B" } else { "A" }.to_string();
    if side == "A" {
        // A面から録り直すときは、前のレコードを片付けて新しく始める
        reset_project(&state);
    }
    let file = format!("recording-{}.wav", side.to_lowercase());
    let settings = state.settings.lock().unwrap().clone();
    let config = RecorderConfig {
        device_id: settings.device_id.clone(),
        path: state.project_dir.join(&file),
        max_seconds: settings.max_seconds(),
        silence_seconds: settings.silence_seconds.clamp(5, 120),
    };
    // 録音中にパソコンが眠らないようにする
    let awake = keepawake::Builder::default()
        .idle(true)
        .sleep(true)
        .reason("レコードを録音しています")
        .app_name("レコード録音")
        .app_reverse_domain("io.github.bunnmei.recordtocd")
        .create()
        .ok();

    let level_app = app.clone();
    let finish_app = app.clone();
    let finish_side = side.clone();
    let handle = recorder::start(
        config,
        move |level| {
            let _ = level_app.emit("rec-level", level);
        },
        move |result: RecordingResult| {
            drop(awake);
            let state = finish_app.state::<AppState>();
            let seconds = result.frames as f64 / result.sample_rate.max(1) as f64;
            let mut error = result.error.clone();
            if result.frames > 0 {
                let mut project = state.project.lock().unwrap().clone();
                state.analyses.lock().unwrap().remove(&file);
                project.put_side(Side {
                    label: finish_side.clone(),
                    file: file.clone(),
                    sample_rate: result.sample_rate,
                    frames: result.frames,
                });
                if project.album_title.is_empty() {
                    project.album_title = default_album_title();
                }
                let index = project
                    .sides
                    .iter()
                    .position(|s| s.label == finish_side)
                    .unwrap_or(0);
                if let Err(e) = state.auto_split_side(&mut project, index) {
                    error.get_or_insert(e);
                }
                *state.project.lock().unwrap() = project.clone();
                state.save_project(&project);
            } else {
                let _ = std::fs::remove_file(state.project_dir.join(&file));
            }
            let _ = finish_app.emit(
                "rec-finished",
                RecordingFinished {
                    side: finish_side,
                    reason: result.reason,
                    seconds,
                    error,
                },
            );
        },
    )?;
    *slot = Some(handle);
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<AppState>) {
    if let Some(handle) = state.recorder.lock().unwrap().as_ref() {
        handle.request_stop();
    }
}

// ───────── 保存・CD ─────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    /// "save" / "prepare" / "burn"
    phase: String,
    /// 0.0〜1.0
    progress: f64,
    track: usize,
    tracks: usize,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl<'a> BusyGuard<'a> {
    fn take(flag: &'a AtomicBool) -> CmdResult<Self> {
        if flag.swap(true, Ordering::SeqCst) {
            Err(friendly_lock_error())
        } else {
            Ok(Self(flag))
        }
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn track_sources(state: &AppState, project: &Project) -> CmdResult<Vec<TrackSource>> {
    let tracks = state.tracks(project)?;
    let offsets = project.side_offsets();
    Ok(tracks
        .iter()
        .map(|t| {
            let side = &project.sides[t.side];
            let rate = side.sample_rate as f64;
            TrackSource {
                path: project.side_path(&state.project_dir, t.side),
                start_frame: ((t.start - offsets[t.side]) * rate).round() as u64,
                end_frame: ((t.end - offsets[t.side]) * rate).round() as u64,
            }
        })
        .collect())
}

fn track_file_name(project: &Project, index: usize) -> String {
    let title = project
        .titles
        .get(index)
        .map(|t| export::safe_file_name(t))
        .unwrap_or_default();
    if title.is_empty() {
        format!("{:02}", index + 1)
    } else {
        format!("{:02} {title}", index + 1)
    }
}

fn unique_dir(base: PathBuf) -> PathBuf {
    if !base.exists() {
        return base;
    }
    let name = base
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    (2..)
        .map(|n| base.with_file_name(format!("{name} ({n})")))
        .find(|p| !p.exists())
        .unwrap()
}

/// 曲ごとに WAV（と MP3）をパソコンに保存する。保存したフォルダを返す。
#[tauri::command]
async fn save_files(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let _busy = BusyGuard::take(&state.busy)?;
    let project = state.project.lock().unwrap().clone();
    let settings = state.settings.lock().unwrap().clone();
    let sources = track_sources(&state, &project)?;
    if sources.is_empty() {
        return Err("保存する曲がありません。".into());
    }
    let album = export::safe_file_name(&project.album_title);
    let album = if album.is_empty() {
        default_album_title()
    } else {
        album
    };
    let dir = unique_dir(settings.save_dir().join(album));
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!("保存先のフォルダを作れませんでした。設定で保存先を確かめてください。（{e}）")
    })?;
    let dir_for_task = dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let total = sources.len();
        for (i, source) in sources.iter().enumerate() {
            let name = track_file_name(&project, i);
            let mut sinks: Vec<Box<dyn Sink>> = vec![Box::new(WavSink::create(
                &dir_for_task.join(format!("{name}.wav")),
                false,
            )?)];
            if settings.save_mp3 {
                sinks.push(Box::new(Mp3Sink::create(
                    &dir_for_task.join(format!("{name}.mp3")),
                    settings.mp3_bitrate,
                )?));
            }
            export::render_track(source, sinks, &mut |p| {
                let _ = app.emit(
                    "work-progress",
                    Progress {
                        phase: "save".into(),
                        progress: (i as f64 + p) / total as f64,
                        track: i + 1,
                        tracks: total,
                    },
                );
            })?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
async fn check_disc() -> DiscStatus {
    tauri::async_runtime::spawn_blocking(burn::check_disc)
        .await
        .unwrap_or(DiscStatus::Unknown {
            message: "CDの状態を確かめられませんでした。".into(),
        })
}

/// 保存先に、このレコード用の新しいフォルダを作る
fn new_save_dir(settings: &Settings, project: &Project) -> CmdResult<PathBuf> {
    let album = export::safe_file_name(&project.album_title);
    let album = if album.is_empty() {
        default_album_title()
    } else {
        album
    };
    let dir = unique_dir(settings.save_dir().join(album));
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!("保存先のフォルダを作れませんでした。設定で保存先を確かめてください。（{e}）")
    })?;
    Ok(dir)
}

/// 録音した音を、曲に分けずにそのまま保存する（A面・B面それぞれ1つのファイル）
#[tauri::command]
async fn save_recording(state: State<'_, AppState>) -> CmdResult<String> {
    let _busy = BusyGuard::take(&state.busy)?;
    let project = state.project.lock().unwrap().clone();
    if project.sides.is_empty() {
        return Err("保存する録音がありません。".into());
    }
    let settings = state.settings.lock().unwrap().clone();
    let dir = new_save_dir(&settings, &project)?;
    for (i, side) in project.sides.iter().enumerate() {
        std::fs::copy(
            project.side_path(&state.project_dir, i),
            dir.join(format!("{}面.wav", side.label)),
        )
        .map_err(|e| {
            format!("ファイルを保存できませんでした。保存先の空き容量を確かめてください。（{e}）")
        })?;
    }
    Ok(dir.to_string_lossy().into_owned())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioFile {
    path: String,
    name: String,
    /// 読めなかったときは None
    duration: Option<f64>,
    error: Option<String>,
}

/// 選ばれた音声ファイルの長さを調べる（CDに入れる曲を選ぶとき）
#[tauri::command]
async fn probe_files(paths: Vec<String>) -> Vec<AudioFile> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let p = Path::new(&path);
                let name = p
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let (duration, error) = match audio::decode::duration(p) {
                    Ok(d) => (Some(d), None),
                    Err(e) => (None, Some(e)),
                };
                AudioFile {
                    path,
                    name,
                    duration,
                    error,
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
fn audio_extensions() -> Vec<&'static str> {
    audio::decode::EXTENSIONS.to_vec()
}

/// i 曲目を、出力先に CD 用 WAV として作る関数
type RenderTrack<'a> = &'a dyn Fn(usize, &Path, &mut dyn FnMut(f64)) -> CmdResult<()>;

/// CD用の WAV を作ってから書き込む。`render(i, 出力先, 進み具合)` で i 曲目を作る。
fn render_and_burn(
    app: &AppHandle,
    cd_dir: &Path,
    total: usize,
    render: RenderTrack,
) -> CmdResult<()> {
    let emit = |phase: &str, progress: f64, track: usize| {
        let _ = app.emit(
            "work-progress",
            Progress {
                phase: phase.into(),
                progress,
                track,
                tracks: total,
            },
        );
    };
    let _ = std::fs::remove_dir_all(cd_dir);
    std::fs::create_dir_all(cd_dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut files = Vec::new();
        for i in 0..total {
            let path = cd_dir.join(format!("{:02}.wav", i + 1));
            render(i, &path, &mut |p| {
                emit("prepare", (i as f64 + p) / total as f64, i + 1)
            })?;
            files.push(path);
        }
        emit("burn", 0.0, 1);
        burn::burn(&files, &mut |p| {
            let track = ((p * total as f64).floor() as usize + 1).min(total);
            emit("burn", p, track);
        })
    })();
    let _ = std::fs::remove_dir_all(cd_dir);
    result
}

fn check_cd_limits(tracks: usize, seconds: f64) -> CmdResult<()> {
    if tracks == 0 {
        return Err("CDに入れる曲がありません。".into());
    }
    if tracks > 99 {
        return Err("音楽CDに入れられるのは99曲までです。".into());
    }
    if seconds > burn::CD_MAX_SECS {
        return Err("CD1枚に入りきりません。曲を減らしてください。".into());
    }
    Ok(())
}

/// 曲分けしたレコードで音楽CDを作る
#[tauri::command]
async fn burn_cd(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let _busy = BusyGuard::take(&state.busy)?;
    let project = state.project.lock().unwrap().clone();
    let sources = track_sources(&state, &project)?;
    let view = album_view(&state, &project)?;
    check_cd_limits(sources.len(), view.cd_seconds)?;
    let cd_dir = state.project_dir.join("cd");
    tauri::async_runtime::spawn_blocking(move || {
        render_and_burn(&app, &cd_dir, sources.len(), &|i, path, progress| {
            let sink = WavSink::create(path, true)?;
            export::render_track(&sources[i], vec![Box::new(sink)], progress)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// パソコンにある曲（選んだ順）で音楽CDを作る
#[tauri::command]
async fn burn_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<()> {
    let _busy = BusyGuard::take(&state.busy)?;
    let cd_dir = state.project_dir.join("cd");
    tauri::async_runtime::spawn_blocking(move || {
        let durations = paths
            .iter()
            .map(|p| audio::decode::duration(Path::new(p)))
            .collect::<CmdResult<Vec<f64>>>()?;
        let total: f64 = durations.iter().sum::<f64>()
            + paths.len().saturating_sub(1) as f64 * burn::CD_GAP_SECS;
        check_cd_limits(paths.len(), total)?;
        render_and_burn(&app, &cd_dir, paths.len(), &|i, path, progress| {
            export::render_file_for_cd(Path::new(&paths[i]), path, durations[i], progress)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let settings_file = config_dir.join("settings.json");
            let project_dir = data_dir.join("project");
            std::fs::create_dir_all(&project_dir)?;
            let mut project = Project::load(&project_dir);
            if project.album_title.is_empty() {
                project.album_title = default_album_title();
            }
            if project.sides.is_empty() {
                project.step = "record".into();
            }
            app.manage(AppState {
                settings: Mutex::new(Settings::load(&settings_file)),
                settings_file,
                project_dir,
                project: Mutex::new(project),
                analyses: Mutex::new(HashMap::new()),
                recorder: Mutex::new(None),
                busy: AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_devices,
            default_save_dir,
            get_album,
            update_album,
            auto_split,
            new_album,
            import_audio,
            save_recording,
            probe_files,
            audio_extensions,
            burn_files,
            start_recording,
            stop_recording,
            save_files,
            check_disc,
            burn_cd,
        ])
        .run(tauri::generate_context!())
        .expect("アプリを起動できませんでした");
}
