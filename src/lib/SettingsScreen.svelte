<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type DeviceInfo, type Settings } from "./api";
  import { app } from "./state.svelte";
  import Icon from "./components/Icon.svelte";

  let devices = $state<DeviceInfo[]>([]);
  let searching = $state(false);
  let saveDir = $state("");
  let savedNote = $state(false);
  let error = $state<string | null>(null);

  const s = $derived(app.settings as Settings);

  async function loadDevices() {
    searching = true;
    try {
      devices = await api.listDevices();
    } catch {
      devices = [];
    }
    searching = false;
  }

  onMount(async () => {
    loadDevices();
    saveDir = await api.defaultSaveDir();
  });

  let noteTimer = 0;
  async function set(patch: Partial<Settings>) {
    error = null;
    try {
      await app.saveSettings(patch);
      saveDir = await api.defaultSaveDir();
      savedNote = true;
      clearTimeout(noteTimer);
      noteTimer = window.setTimeout(() => (savedNote = false), 2500);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function chooseFolder() {
    const dir = await open({ directory: true, defaultPath: saveDir, title: "保存する場所を選ぶ" });
    if (typeof dir === "string") await set({ saveDir: dir });
  }

  async function resetAll() {
    const ok = await app.confirm({
      title: "設定を最初の状態にもどしますか？",
      message: "録音した音は消えません。設定だけが最初の状態にもどります。",
      ok: "もどす",
      cancel: "やめる",
    });
    if (!ok) return;
    await set({
      deviceId: null,
      recordType: "lp",
      customMaxMinutes: 30,
      silenceSeconds: 15,
      splitSensitivity: "normal",
      saveMp3: true,
      mp3Bitrate: 192,
      saveDir: null,
      fontSize: "large",
      theme: "auto",
    });
  }

  type Option<T> = { value: T; label: string };
  const themes: Option<Settings["theme"]>[] = [
    { value: "auto", label: "パソコンに合わせる" },
    { value: "light", label: "明るい" },
    { value: "dark", label: "暗い" },
  ];
  const fonts: Option<Settings["fontSize"]>[] = [
    { value: "large", label: "大きい" },
    { value: "xlarge", label: "とても大きい" },
  ];
  const recordTypes: Option<Settings["recordType"]>[] = [
    { value: "lp", label: "LP（大きいレコード・片面30分まで）" },
    { value: "ep", label: "EP・シングル（小さいレコード・片面10分まで）" },
    { value: "custom", label: "自分で決める" },
  ];
  const silences: Option<number>[] = [
    { value: 10, label: "10秒" },
    { value: 15, label: "15秒（おすすめ）" },
    { value: 30, label: "30秒" },
    { value: 60, label: "60秒" },
  ];
  const sensitivities: Option<Settings["splitSensitivity"]>[] = [
    { value: "low", label: "弱い" },
    { value: "normal", label: "ふつう（おすすめ）" },
    { value: "high", label: "強い" },
  ];
  const mp3Options: Option<string>[] = [
    { value: "off", label: "保存しない（WAVだけ）" },
    { value: "192", label: "保存する（ふつうの音質）" },
    { value: "320", label: "保存する（高い音質）" },
  ];
  const mp3Value = $derived(!s.saveMp3 ? "off" : s.mp3Bitrate >= 320 ? "320" : "192");
</script>

<div class="screen">
  <header>
    <button class="btn btn-big back" onclick={() => (app.screen = "main")}><Icon name="back" /> もどる</button>
    <h1>設定</h1>
    <div class="spacer"></div>
    {#if savedNote}
      <span class="saved" role="status"><Icon name="check" /> 保存しました</span>
    {/if}
  </header>

  <main>
    <p class="muted">変えた設定は、すぐに保存されます。わからない項目は、そのままで大丈夫です。</p>

    {#if error}
      <div class="notice error"><span aria-hidden="true">⚠</span><span>{error}</span></div>
    {/if}

    {#snippet choices<T>(name: string, options: Option<T>[], current: T, onPick: (v: T) => void)}
      <div class="choices" role="radiogroup" aria-label={name}>
        {#each options as option (option.label)}
          <button
            class="choice"
            class:on={option.value === current}
            role="radio"
            aria-checked={option.value === current}
            onclick={() => onPick(option.value)}
          >
            <span class="mark" aria-hidden="true">{option.value === current ? "●" : "○"}</span>
            {option.label}
          </button>
        {/each}
      </div>
    {/snippet}

    <section class="card item">
      <h2>画面の明るさ</h2>
      <p class="muted">見やすい方を選んでください。</p>
      {@render choices("画面の明るさ", themes, s.theme, (v) => set({ theme: v }))}
    </section>

    <section class="card item">
      <h2>文字の大きさ</h2>
      {@render choices("文字の大きさ", fonts, s.fontSize, (v) => set({ fontSize: v }))}
    </section>

    <section class="card item">
      <h2>録音に使うレコードプレーヤー</h2>
      <p class="muted">ふつうは「自動で選ぶ」のままで大丈夫です。</p>
      <div class="row">
        <select
          class="input select"
          value={s.deviceId ?? ""}
          onchange={(e) => set({ deviceId: e.currentTarget.value || null })}
          aria-label="録音に使う機器"
        >
          <option value="">自動で選ぶ</option>
          {#each devices as d (d.id)}
            <option value={d.id}>{d.name}{d.recommended ? "（レコードプレーヤー）" : ""}</option>
          {/each}
        </select>
        <button class="btn" disabled={searching} onclick={loadDevices}>
          {searching ? "さがしています…" : "もう一度さがす"}
        </button>
      </div>
    </section>

    <section class="card item">
      <h2>レコードの種類</h2>
      <p class="muted">片面の録音は、この時間になると自動で止まります。</p>
      {@render choices("レコードの種類", recordTypes, s.recordType, (v) => set({ recordType: v }))}
      {#if s.recordType === "custom"}
        <label class="row minutes">
          片面の最大
          <input
            class="input number"
            type="number"
            min="1"
            max="120"
            value={s.customMaxMinutes}
            onchange={(e) => set({ customMaxMinutes: Math.min(120, Math.max(1, Number(e.currentTarget.value) || 30)) })}
          />
          分
        </label>
      {/if}
    </section>

    <section class="card item">
      <h2>音が止まってから、自動で録音を止めるまで</h2>
      <p class="muted">レコードが終わって音がしなくなってから、この時間がたつと録音が止まります。</p>
      {@render choices("自動で止めるまでの時間", silences, s.silenceSeconds, (v) => set({ silenceSeconds: v }))}
    </section>

    <section class="card item">
      <h2>曲の分け方</h2>
      <p class="muted">
        1曲が2つに分かれてしまうときは「弱い」に、曲と曲がつながってしまうときは「強い」にしてください。
        変えたあとは、曲を分ける画面で「自動で分けなおす」を押してください。
      </p>
      {@render choices("曲の分け方", sensitivities, s.splitSensitivity, (v) => set({ splitSensitivity: v }))}
    </section>

    <section class="card item">
      <h2>MP3も保存する</h2>
      <p class="muted">MP3は、スマートフォンや音楽プレーヤーで聴くときに便利です。WAVはいつも保存されます。</p>
      {@render choices("MP3の保存", mp3Options, mp3Value, (v) =>
        set(v === "off" ? { saveMp3: false } : { saveMp3: true, mp3Bitrate: Number(v) }),
      )}
    </section>

    <section class="card item">
      <h2>保存する場所</h2>
      <p class="path">{saveDir}</p>
      <div class="row">
        <button class="btn" onclick={chooseFolder}>場所を変える</button>
        {#if s.saveDir}
          <button class="btn" onclick={() => set({ saveDir: null })}>最初の場所にもどす</button>
        {/if}
      </div>
    </section>

    <div class="row reset">
      <button class="btn" onclick={resetAll}>設定を最初の状態にもどす</button>
    </div>
  </main>
</div>

<style>
  .screen {
    min-height: 100vh;
  }
  header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 1.2rem;
    padding: 0.7rem 1.4rem;
    background: var(--header-bg);
    color: var(--header-text);
    box-shadow: 0 2px 14px rgba(0, 0, 0, 0.25);
  }
  .back {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.45);
    color: #fff;
    min-height: 3rem;
    font-size: 1.1rem;
    padding: 0.3rem 1.4rem;
  }
  h1 {
    font-size: 1.5rem;
  }
  .saved {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-weight: 800;
    color: #8ff0a4;
  }
  main {
    max-width: 56rem;
    margin: 0 auto;
    padding: 1.4rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .item h2 {
    font-size: 1.2rem;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 2.8rem;
    padding: 0.3rem 1.1rem;
    border: 2px solid var(--border);
    border-radius: 0.6rem;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 1rem;
    cursor: pointer;
  }
  .choice:hover {
    background: var(--surface-2);
  }
  .choice.on {
    border-color: var(--primary);
    border-width: 3px;
    font-weight: 800;
  }
  .choice.on .mark {
    color: var(--primary);
  }
  .select {
    flex: 1;
    min-width: 16rem;
  }
  .minutes {
    font-weight: 700;
  }
  .number {
    width: 6rem;
  }
  .path {
    font-weight: 700;
    word-break: break-all;
  }
  .reset {
    justify-content: flex-end;
    padding-bottom: 2rem;
  }
</style>
