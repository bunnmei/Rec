import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  deviceId: string | null;
  recordType: "lp" | "ep" | "custom";
  customMaxMinutes: number;
  silenceSeconds: number;
  splitSensitivity: "low" | "normal" | "high";
  saveMp3: boolean;
  mp3Bitrate: number;
  saveDir: string | null;
  fontSize: "large" | "xlarge";
  theme: "auto" | "light" | "dark";
}

export interface DeviceInfo {
  id: string;
  name: string;
  recommended: boolean;
}

export interface SideView {
  label: string;
  path: string;
  offset: number;
  duration: number;
  blockSecs: number;
  peaks: number[];
}

export interface Track {
  side: number;
  start: number;
  end: number;
}

export type Step = "record" | "split" | "output";

export interface Album {
  sides: SideView[];
  tracks: Track[];
  splits: number[];
  titles: string[];
  albumTitle: string;
  step: Step;
  imported: boolean;
  cdSeconds: number;
  cdMaxSeconds: number;
}

export type RecState = "waiting" | "recording" | "quiet";

export interface LevelInfo {
  state: RecState;
  peakLeft: number;
  peakRight: number;
  elapsed: number;
  quietSeconds: number;
  silenceSeconds: number;
  maxSeconds: number;
  waitingSeconds: number;
  clipping: boolean;
}

export interface RecordingFinished {
  side: "A" | "B";
  reason: "silence" | "maxTime" | "manual" | "error";
  seconds: number;
  error: string | null;
}

export type DiscStatus =
  | { status: "noTool" }
  | { status: "noDrive" }
  | { status: "noDisc" }
  | { status: "notBlank" }
  | { status: "wrongMedia" }
  | { status: "ready" }
  | { status: "unknown"; message: string };

export interface Progress {
  phase: "save" | "prepare" | "burn";
  progress: number;
  track: number;
  tracks: number;
}

export interface AudioFile {
  path: string;
  name: string;
  duration: number | null;
  error: string | null;
}

export interface AlbumUpdate {
  splits?: number[];
  titles?: string[];
  albumTitle?: string;
  step?: Step;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  listDevices: () => invoke<DeviceInfo[]>("list_devices"),
  defaultSaveDir: () => invoke<string>("default_save_dir"),
  getAlbum: () => invoke<Album | null>("get_album"),
  updateAlbum: (update: AlbumUpdate) => invoke<Album>("update_album", { update }),
  autoSplit: () => invoke<Album>("auto_split"),
  newAlbum: () => invoke<void>("new_album"),
  importAudio: (path: string) => invoke<Album>("import_audio", { path }),
  saveRecording: () => invoke<string>("save_recording"),
  probeFiles: (paths: string[]) => invoke<AudioFile[]>("probe_files", { paths }),
  audioExtensions: () => invoke<string[]>("audio_extensions"),
  burnFiles: (paths: string[]) => invoke<void>("burn_files", { paths }),
  startRecording: (side: "A" | "B") => invoke<void>("start_recording", { side }),
  stopRecording: () => invoke<void>("stop_recording"),
  saveFiles: () => invoke<string>("save_files"),
  checkDisc: () => invoke<DiscStatus>("check_disc"),
  burnCd: () => invoke<void>("burn_cd"),
};

/** 「3分42秒」のような、年配の方にも読みやすい時間の書き方 */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}時間${m}分`;
  if (m > 0) return `${m}分${s}秒`;
  return `${s}秒`;
}

/** 「12:34」のような、細かい位置の表示 */
export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** CD の曲と曲のあいだの無音（秒） */
export const CD_GAP = 2;
/** CD-R に入る最大の長さ（秒） */
export const CD_MAX = 79 * 60 + 57;

/** 振幅（0〜1）を、波形表示用の 0〜255 にする（-60dB〜0dB） */
export function peakByte(amplitude: number): number {
  const db = 20 * Math.log10(Math.max(amplitude, 1e-6));
  return Math.round(Math.min(1, Math.max(0, (db + 60) / 60)) * 255);
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "うまくいきませんでした。もう一度お試しください。";
}
