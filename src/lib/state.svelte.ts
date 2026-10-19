import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  errorText,
  peakByte,
  type Album,
  type AlbumUpdate,
  type LevelInfo,
  type Progress,
  type RecordingFinished,
  type Settings,
} from "./api";

export interface DialogRequest {
  title: string;
  message: string;
  ok: string;
  cancel: string;
  /** 取り消せない操作は赤いボタンにする */
  danger?: boolean;
  resolve: (ok: boolean) => void;
}

export type Task = "save" | "saveRaw" | "burn";

class AppStore {
  screen = $state<"main" | "settings">("main");
  settings = $state<Settings | null>(null);
  /** 作業中のレコード（録音したもの、または読み込んだもの） */
  album = $state<Album | null>(null);

  /** 録音中の面。録音していなければ null */
  recording = $state<"A" | "B" | null>(null);
  level = $state<LevelInfo | null>(null);
  /** 録音中の波形（0.05秒ごとの音の大きさ 0〜255）。長くなるので、変化は liveTick で知らせる */
  livePeaks: number[] = [];
  liveTick = $state(0);
  lastFinished = $state<RecordingFinished | null>(null);

  /** 保存・CD作成の進み具合 */
  task = $state<Task | null>(null);
  progress = $state<Progress | null>(null);

  dialog = $state<DialogRequest | null>(null);
  extensions = $state<string[]>(["wav", "mp3", "flac", "m4a"]);

  get busy() {
    return this.task !== null || this.recording !== null;
  }

  async init() {
    this.settings = await api.getSettings();
    this.extensions = await api.audioExtensions().catch(() => this.extensions);
    await listen<LevelInfo>("rec-level", (e) => {
      this.level = e.payload;
      if (e.payload.state !== "waiting") {
        this.livePeaks.push(peakByte(Math.max(e.payload.peakLeft, e.payload.peakRight)));
        this.liveTick++;
      }
    });
    await listen<RecordingFinished>("rec-finished", async (e) => {
      this.recording = null;
      this.level = null;
      this.lastFinished = e.payload;
      await this.refreshAlbum();
    });
    await listen<Progress>("work-progress", (e) => {
      this.progress = e.payload;
    });
    await this.refreshAlbum();
  }

  async refreshAlbum() {
    try {
      this.album = await api.getAlbum();
    } catch (e) {
      console.error(errorText(e));
      this.album = null;
    }
  }

  async updateAlbum(update: AlbumUpdate) {
    this.album = await api.updateAlbum(update);
  }

  async saveSettings(patch: Partial<Settings>) {
    if (!this.settings) return;
    this.settings = await api.saveSettings({ ...this.settings, ...patch });
  }

  /** 時間のかかる作業（保存・CD作成）を、進み具合の表示つきで行う */
  async run<T>(task: Task, work: () => Promise<T>): Promise<T> {
    this.task = task;
    this.progress = null;
    try {
      return await work();
    } finally {
      this.task = null;
      this.progress = null;
    }
  }

  /** 音声ファイルを選ぶ小窓を開く */
  async pickAudio(multiple: false): Promise<string | null>;
  async pickAudio(multiple: true): Promise<string[]>;
  async pickAudio(multiple: boolean): Promise<string | string[] | null> {
    const exts = [...this.extensions, ...this.extensions.map((e) => e.toUpperCase())];
    const picked = await open({
      multiple,
      title: multiple ? "CDに入れる曲を選ぶ（いくつでも選べます）" : "読み込む音声ファイルを選ぶ",
      filters: [{ name: "音声ファイル", extensions: exts }],
    });
    if (multiple) return Array.isArray(picked) ? picked : picked ? [picked] : [];
    return typeof picked === "string" ? picked : null;
  }

  /** 確認の小窓を出し、押されたボタンを返す */
  confirm(options: Omit<DialogRequest, "resolve">): Promise<boolean> {
    return new Promise((resolve) => {
      this.dialog = {
        ...options,
        resolve: (ok) => {
          this.dialog = null;
          resolve(ok);
        },
      };
    });
  }
}

export const app = new AppStore();

/** 画面の中の区画へ、ゆっくり移動する */
export function scrollToSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}
