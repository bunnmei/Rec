<script lang="ts">
  import { onMount } from "svelte";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { api, errorText, formatClock, formatDuration, type DeviceInfo } from "./api";
  import { app, scrollToSection } from "./state.svelte";
  import Section from "./components/Section.svelte";
  import Icon from "./components/Icon.svelte";
  import LevelMeter from "./components/LevelMeter.svelte";
  import LiveWave from "./components/LiveWave.svelte";

  let device = $state<DeviceInfo | null>(null);
  let devicesChecked = $state(false);
  let error = $state<string | null>(null);
  let starting = $state(false);
  /** 「B面は録音しない」を選んだ */
  let skipB = $state(false);
  let savedDir = $state<string | null>(null);

  const album = $derived(app.album);
  const recorded = $derived(album && !album.imported ? album.sides : []);
  const sideA = $derived(recorded.find((s) => s.label === "A"));
  const sideB = $derived(recorded.find((s) => s.label === "B"));
  const level = $derived(app.level);
  const finished = $derived(app.lastFinished);
  const done = $derived(!!sideA && (!!sideB || skipB));

  async function findDevice() {
    devicesChecked = false;
    try {
      const list = await api.listDevices();
      const chosen = app.settings?.deviceId;
      device =
        list.find((d) => d.id === chosen) ?? list.find((d) => d.recommended) ?? list[0] ?? null;
    } catch {
      device = null;
    }
    devicesChecked = true;
  }

  onMount(findDevice);

  async function start(side: "A" | "B") {
    if (side === "A" && album) {
      const ok = await app.confirm({
        title: "新しく録音しますか？",
        message: album.imported
          ? "いま使っている読み込んだファイルと、曲の区切りは消えて、新しく録音を始めます。（元のファイルは消えません）"
          : "いま録音してあるA面・B面の音は消えて、新しく録音を始めます。",
        ok: "消して録音する",
        cancel: "やめる",
        danger: true,
      });
      if (!ok) return;
    }
    if (side === "B" && sideB) {
      const ok = await app.confirm({
        title: "B面を録音しなおしますか？",
        message: "いま録音してあるB面の音は消えて、新しく録音を始めます。",
        ok: "録音しなおす",
        cancel: "やめる",
        danger: true,
      });
      if (!ok) return;
    }
    error = null;
    savedDir = null;
    starting = true;
    app.lastFinished = null;
    try {
      await api.startRecording(side);
      app.recording = side;
      app.level = null;
      app.livePeaks = [];
      app.liveTick++;
      skipB = false;
      if (side === "A") await app.refreshAlbum();
    } catch (e) {
      error = errorText(e);
    } finally {
      starting = false;
    }
  }

  async function stop() {
    const waiting = !level || level.state === "waiting";
    if (!waiting) {
      const ok = await app.confirm({
        title: "録音を止めますか？",
        message: "曲が終わると自動で止まるので、ふつうは押さなくて大丈夫です。\nここまでの録音は残ります。",
        ok: "止める",
        cancel: "録音をつづける",
      });
      if (!ok) return;
    }
    await api.stopRecording();
  }

  async function saveRaw() {
    error = null;
    try {
      savedDir = await app.run("saveRaw", api.saveRecording);
    } catch (e) {
      error = errorText(e);
    }
  }

  function reasonText(reason: string) {
    switch (reason) {
      case "silence":
        return "音が止まったので、自動で録音を終えました。";
      case "maxTime":
        return "最大の録音時間になったので、録音を終えました。";
      default:
        return "録音を止めました。";
    }
  }
</script>

<Section
  id="record"
  number={1}
  icon="record"
  tone="record"
  title="レコードを録音する"
  lead="針を落とすと自動で録音が始まり、レコードが終わると自動で止まります。"
  status={done ? "録音済み" : undefined}
>
  {#if app.recording}
    {@const waiting = !level || level.state === "waiting"}
    <div class="display" class:waiting>
      <div class="display-top">
        {#if waiting}
          <span class="badge standby"><span class="dot"></span>待機中</span>
          <span class="display-msg">レコードに針を落としてください。音が聞こえたら自動で{app.recording}面の録音が始まります。</span>
        {:else}
          <span class="badge rec"><span class="dot"></span>REC</span>
          <span class="display-msg">{app.recording}面を録音しています</span>
        {/if}
      </div>
      <div class="clock">
        {formatClock(level?.elapsed ?? 0)}
        {#if level && !waiting}
          <span class="clock-max">/ 最大 {formatClock(level.maxSeconds)}</span>
        {/if}
      </div>
      <LiveWave peaks={app.livePeaks} tick={app.liveTick} recording={!waiting} />
      <LevelMeter left={level?.peakLeft ?? 0} right={level?.peakRight ?? 0} />
      <p class="display-note">
        {#if waiting && level && level.waitingSeconds > 60}
          <Icon name="alert" /> まだ音が届いていません。プレーヤーの電源と、USBケーブルを確かめてください。
        {:else if waiting}
          針を落とすまで、ゆっくりで大丈夫です。
        {:else if level?.state === "quiet"}
          音が止まっています。このまま音がなければ、<strong
            >あと {Math.max(0, Math.ceil(level.silenceSeconds - level.quietSeconds))} 秒</strong
          >で自動で止まります。
        {:else}
          レコードが終わると自動で止まります。そのままお待ちください。
        {/if}
      </p>
      {#if level?.clipping}
        <p class="display-warn"><Icon name="alert" /> 音が大きすぎて割れています。プレーヤーに音量のつまみがあれば、少し下げてください。</p>
      {/if}
      <div class="row">
        <button class="btn btn-big stop" onclick={stop}>
          {#if waiting}<Icon name="close" /> やめる{:else}<Icon name="stop" /> 録音を止める{/if}
        </button>
      </div>
    </div>
  {:else if !sideA}
    {#if album?.imported}
      <div class="notice info">
        <Icon name="file" />
        <span>いまは、パソコンから読み込んだファイル「{album.albumTitle}」を使っています。新しく録音するときは、下のボタンを押してください。</span>
      </div>
    {/if}
    <ol class="howto">
      <li><span class="n">1</span><span>レコードを<strong>A面</strong>を上にして、プレーヤーにのせます</span></li>
      <li><span class="n">2</span><span>下の<strong>「A面の録音をはじめる」</strong>を押します</span></li>
      <li><span class="n">3</span><span>針を落とします。<strong>あとは待つだけです</strong></span></li>
    </ol>
    {#if finished && finished.seconds === 0 && !finished.error}
      <div class="notice"><Icon name="info" /><span>録音されませんでした。もう一度「A面の録音をはじめる」を押してください。</span></div>
    {/if}
    <div class="row">
      <button class="btn btn-big btn-record" disabled={starting || !device || app.busy} onclick={() => start("A")}>
        <Icon name="record" /> A面の録音をはじめる
      </button>
    </div>
    <div class="device">
      {#if !devicesChecked}
        <span class="muted">レコードプレーヤーをさがしています…</span>
      {:else if device}
        <span class="ok"><Icon name="check" /></span>
        <span>使うプレーヤー：<strong>{device.name}</strong></span>
        {#if !device.recommended}
          <span class="muted">（レコードプレーヤーではないかもしれません。設定で選べます）</span>
        {/if}
      {:else}
        <div class="notice">
          <Icon name="alert" />
          <span>レコードプレーヤーが見つかりません。USBケーブルをつないで、プレーヤーの電源を入れてから「もう一度さがす」を押してください。</span>
        </div>
        <button class="btn" onclick={findDevice}>もう一度さがす</button>
      {/if}
    </div>
  {:else if !done}
    <div class="done-line">
      <Icon name="check" /> A面を録音しました（{formatDuration(sideA.duration)}）
      {#if finished && finished.side === "A" && !finished.error}
        <span class="muted reason">— {reasonText(finished.reason)}</span>
      {/if}
    </div>
    <div class="flip">
      <div class="flip-icon"><Icon name="flip" size="2.6rem" /></div>
      <div>
        <h3>つぎは B面 です</h3>
        <p>レコードを<strong>裏返して</strong>のせ、「B面の録音をはじめる」を押してから針を落とします。</p>
      </div>
    </div>
    {#if finished && finished.side === "B" && finished.seconds === 0 && !finished.error}
      <div class="notice"><Icon name="info" /><span>B面は録音されませんでした。録音するときは、もう一度「B面の録音をはじめる」を押してください。</span></div>
    {/if}
    <div class="row">
      <button class="btn btn-big btn-record" disabled={starting || app.busy} onclick={() => start("B")}>
        <Icon name="record" /> B面の録音をはじめる
      </button>
      <button class="btn btn-big" disabled={app.busy} onclick={() => (skipB = true)}>B面は録音しない</button>
    </div>
    <div class="sub row">
      <button class="btn btn-small" disabled={app.busy} onclick={() => start("A")}>A面を録音しなおす</button>
    </div>
  {:else}
    <div class="done-line">
      <Icon name="check" />
      {#if sideB}
        A面（{formatDuration(sideA.duration)}）と B面（{formatDuration(sideB.duration)}）を録音しました
      {:else}
        A面（{formatDuration(sideA.duration)}）を録音しました
      {/if}
    </div>
    <p>つぎは「② 曲を分ける」で、曲ごとに分けます。録音したままの音を保存することもできます。</p>
    <div class="row">
      <button class="btn btn-big btn-primary" onclick={() => scrollToSection("split")}>
        <Icon name="arrowDown" /> ② 曲を分けるへ
      </button>
      <button class="btn btn-big" disabled={app.busy} onclick={saveRaw}>
        <Icon name="save" /> 録音したまま保存する
      </button>
    </div>
    {#if savedDir}
      <div class="notice success">
        <Icon name="check" />
        <span>保存しました。</span>
        <span class="spacer"></span>
        <button class="btn btn-small" onclick={() => savedDir && openPath(savedDir)}><Icon name="folder" /> フォルダを開く</button>
      </div>
    {/if}
    <div class="sub row">
      {#if !sideB}
        <button class="btn btn-small" disabled={app.busy} onclick={() => start("B")}>やっぱりB面も録音する</button>
      {:else}
        <button class="btn btn-small" disabled={app.busy} onclick={() => start("B")}>B面を録音しなおす</button>
      {/if}
      <button class="btn btn-small" disabled={app.busy} onclick={() => start("A")}>最初から録音しなおす</button>
    </div>
  {/if}

  {#if error}
    <div class="notice error"><Icon name="alert" /><span>{error}</span></div>
  {/if}
  {#if finished?.error}
    <div class="notice error"><Icon name="alert" /><span>{finished.error}</span></div>
  {/if}
</Section>

<style>
  .howto {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.8rem;
  }
  @media (max-width: 900px) {
    .howto {
      grid-template-columns: 1fr;
    }
  }
  .howto li {
    display: flex;
    gap: 0.7rem;
    align-items: flex-start;
    padding: 0.9rem 1rem;
    border-radius: 0.9rem;
    background: var(--surface-3);
    border: 1px solid var(--line);
  }
  .n {
    flex: none;
    display: grid;
    place-items: center;
    width: 1.8rem;
    height: 1.8rem;
    border-radius: 50%;
    background: var(--record);
    color: var(--on-primary);
    font-weight: 900;
  }
  .device {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    padding-top: 0.8rem;
    border-top: 1px solid var(--line);
  }
  .ok {
    color: var(--success);
  }
  .done-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    font-size: 1.15rem;
    font-weight: 800;
    color: var(--success);
  }
  .reason {
    font-size: 0.95rem;
    font-weight: 400;
  }
  .flip {
    display: flex;
    gap: 1.1rem;
    align-items: center;
    padding: 1rem 1.2rem;
    border-radius: 1rem;
    background: var(--record-soft);
  }
  .flip-icon {
    color: var(--record);
  }
  .flip h3 {
    font-size: 1.4rem;
  }
  .sub {
    padding-top: 0.8rem;
    border-top: 1px solid var(--line);
  }

  /* ───── 録音中の「録音機の画面」 ───── */
  .display {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    padding: 1.4rem 1.6rem;
    border-radius: 1.1rem;
    background: var(--display-bg);
    color: var(--display-text);
    box-shadow:
      inset 0 0 0 1px var(--display-grid),
      var(--shadow-strong);
  }
  .display-top {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    flex-wrap: wrap;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.15rem 0.8rem;
    border-radius: 0.4rem;
    font-weight: 900;
    letter-spacing: 0.12em;
  }
  .badge.rec {
    background: var(--display-rec);
    color: #fff;
  }
  .badge.standby {
    background: var(--display-grid);
    color: var(--display-text);
  }
  .dot {
    width: 0.7rem;
    height: 0.7rem;
    border-radius: 50%;
    background: currentColor;
  }
  .badge.rec .dot {
    animation: blink 1.4s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }
  .display-msg {
    font-size: 1.15rem;
    font-weight: 700;
  }
  .clock {
    font-size: 3.6rem;
    font-weight: 800;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.03em;
  }
  .clock-max {
    font-size: 1rem;
    font-weight: 600;
    color: var(--display-muted);
    letter-spacing: 0;
  }
  .display-note {
    color: var(--display-text);
  }
  .display-warn {
    color: #ffd166;
    font-weight: 800;
  }
  .stop {
    background: transparent;
    color: var(--display-text);
    border-color: var(--display-muted);
  }
  .stop:hover:not(:disabled) {
    background: var(--display-grid);
  }
</style>
