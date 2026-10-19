<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api, errorText, formatClock, formatDuration, type Album } from "./api";
  import { app } from "./state.svelte";
  import Waveform from "./components/Waveform.svelte";
  import Icon from "./components/Icon.svelte";

  /** 曲の長さの最短（秒）。CD の決まりで 4 秒より短い曲は作れない。 */
  const MIN_TRACK = 4;
  const NUDGE = 0.5;

  const album = $derived(app.album as Album);

  // 動かせる区切り（同じ面の中の、曲と曲のあいだ）
  interface SplitInfo {
    /** 区切りの前の曲の番号 */
    track: number;
    min: number;
    max: number;
  }
  const splitInfo = $derived.by(() => {
    const t = album.tracks;
    const list: SplitInfo[] = [];
    for (let k = 0; k < t.length - 1; k++) {
      if (t[k].side === t[k + 1].side) {
        list.push({ track: k, min: t[k].start + MIN_TRACK, max: t[k + 1].end - MIN_TRACK });
      }
    }
    return list;
  });

  let splits = $state<number[]>([]);
  let titles = $state<string[]>([]);
  let selected = $state<number | null>(null);
  let message = $state<string | null>(null);
  let history = $state<{ splits: number[]; titles: string[] }[]>([]);

  function sync() {
    splits = splitInfo.map((info) => album.tracks[info.track].end);
    titles = album.tracks.map((_, i) => album.titles[i] ?? "");
    if (selected !== null && selected >= splits.length) selected = null;
  }
  // 録音が終わった・ファイルを読み込んだなど、外で変わったときも合わせる
  $effect.pre(() => {
    void app.album;
    untrack(sync);
  });

  async function commit(newSplits: number[], newTitles: string[], remember = true) {
    if (remember) {
      history = [...history, { splits: [...splits], titles: [...titles] }].slice(-50);
    }
    message = null;
    try {
      await app.updateAlbum({ splits: newSplits, titles: newTitles });
    } catch (e) {
      message = errorText(e);
    }
    sync();
  }

  // ───── 再生 ─────
  let audio: HTMLAudioElement | undefined = $state();
  let playing = $state(false);
  let playhead = $state(0);
  let currentSide = -1;
  let stopAt: number | null = null;
  let playingTrack = $state<number | null>(null);
  let raf = 0;

  function sideIndexAt(t: number) {
    let index = 0;
    album.sides.forEach((s, i) => {
      if (t >= s.offset) index = i;
    });
    return index;
  }

  function tick() {
    if (!audio || currentSide < 0) return;
    playhead = album.sides[currentSide].offset + audio.currentTime;
    if (stopAt !== null && playhead >= stopAt) {
      pause();
      return;
    }
    if (playing) raf = requestAnimationFrame(tick);
  }

  async function playFrom(t: number, until: number | null = null, track: number | null = null) {
    if (!audio) return;
    const index = sideIndexAt(t);
    const side = album.sides[index];
    if (currentSide !== index) {
      audio.src = convertFileSrc(side.path);
      currentSide = index;
      await new Promise<void>((resolve) => {
        const done = () => {
          audio?.removeEventListener("loadedmetadata", done);
          resolve();
        };
        audio?.addEventListener("loadedmetadata", done);
        audio?.load();
      });
    }
    audio.currentTime = Math.max(0, t - side.offset);
    playhead = t;
    stopAt = until;
    playingTrack = track;
    try {
      await audio.play();
      playing = true;
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(tick);
    } catch (e) {
      message = "音を再生できませんでした。" + errorText(e);
    }
  }

  function pause() {
    audio?.pause();
    playing = false;
    playingTrack = null;
    stopAt = null;
    cancelAnimationFrame(raf);
  }

  onDestroy(pause);

  // ───── 区切りの操作 ─────
  function select(i: number) {
    selected = i;
    pause();
    playhead = splits[i];
  }

  function move(i: number, t: number) {
    const info = splitInfo[i];
    splits[i] = Math.min(info.max, Math.max(info.min, t));
    playhead = splits[i];
  }

  function nudge(delta: number) {
    if (selected === null) return;
    const before = splits[selected];
    move(selected, before + delta);
    const moved = [...splits];
    splits[selected] = before;
    commit(moved, titles);
  }

  function addSplit() {
    const t = playhead;
    const k = album.tracks.findIndex((tr) => t > tr.start && t < tr.end);
    if (k < 0 || t - album.tracks[k].start < MIN_TRACK || album.tracks[k].end - t < MIN_TRACK) {
      message = "曲の始まりや終わりに近すぎるので、ここでは区切れません。";
      return;
    }
    const newTitles = [...titles];
    newTitles.splice(k + 1, 0, "");
    const newSplits = [...splits, t].sort((a, b) => a - b);
    commit(newSplits, newTitles).then(() => {
      selected = splits.findIndex((s) => Math.abs(s - t) < 0.05);
    });
  }

  async function removeSplit() {
    if (selected === null) return;
    const k = splitInfo[selected].track;
    const newTitles = [...titles];
    newTitles.splice(k, 2, titles[k] || titles[k + 1] || "");
    const newSplits = splits.filter((_, i) => i !== selected);
    selected = null;
    await commit(newSplits, newTitles);
  }

  async function undo() {
    const last = history[history.length - 1];
    if (!last) return;
    history = history.slice(0, -1);
    selected = null;
    await commit(last.splits, last.titles, false);
  }

  async function autoSplitAgain() {
    const ok = await app.confirm({
      title: "自動で分けなおしますか？",
      message: "いまの区切りと曲名は消えて、もう一度自動で曲を分けます。\n（「元に戻す」で戻せます）",
      ok: "分けなおす",
      cancel: "やめる",
    });
    if (!ok) return;
    history = [...history, { splits: [...splits], titles: [...titles] }];
    selected = null;
    try {
      app.album = await api.autoSplit();
    } catch (e) {
      message = errorText(e);
    }
    sync();
  }

  // 曲名は打つたびに保存する（少し待ってから）
  let titleTimer = 0;
  function onTitle(i: number, value: string) {
    titles[i] = value;
    clearTimeout(titleTimer);
    titleTimer = window.setTimeout(() => app.updateAlbum({ titles: [...titles] }), 500);
  }
  let albumTimer = 0;
  function onAlbumTitle(value: string) {
    clearTimeout(albumTimer);
    albumTimer = window.setTimeout(() => app.updateAlbum({ albumTitle: value }), 500);
  }

  const selectedLabel = $derived(
    selected !== null && splitInfo[selected]
      ? `${splitInfo[selected].track + 1}曲目と${splitInfo[selected].track + 2}曲目のあいだ`
      : "",
  );
</script>

<audio bind:this={audio} onended={pause} preload="metadata"></audio>

<div class="split-editor">
  <div class="panel wave-card">
    <Waveform
      {album}
      {splits}
      {playhead}
      {selected}
      onSeek={(t) => {
        selected = null;
        playFrom(t);
      }}
      onSelect={select}
      onMove={move}
      onMoveEnd={() => {
        const moved = [...splits];
        sync();
        commit(moved, titles);
      }}
    />

    <div class="controls row">
      {#if playing}
        <button class="btn" onclick={pause}><Icon name="pause" /> 一時停止</button>
      {:else}
        <button class="btn btn-primary" onclick={() => playFrom(playhead)}><Icon name="play" /> 再生</button>
      {/if}
      <span class="position">いまの位置 <strong>{formatClock(playhead)}</strong></span>
      <button class="btn" onclick={addSplit}><Icon name="cut" /> ここで区切る</button>
      <div class="spacer"></div>
      <button class="btn" disabled={history.length === 0} onclick={undo}><Icon name="undo" /> 元に戻す</button>
      <button class="btn" onclick={autoSplitAgain}><Icon name="magic" /> 自動で分けなおす</button>
    </div>

    {#if selected !== null}
      <div class="selected-box">
        <div class="row">
          <div class="selected-title">選んでいる区切り：<strong>{selectedLabel}</strong></div>
          <div class="spacer"></div>
          <button class="btn btn-small" onclick={() => (selected = null)}><Icon name="close" /> 閉じる</button>
        </div>
        <div class="row">
          <button class="btn" onclick={() => nudge(-NUDGE)}><Icon name="back" /> 少し前へ</button>
          <button class="btn" onclick={() => nudge(NUDGE)}>少し後ろへ <span class="flipx"><Icon name="back" /></span></button>
          <button class="btn" onclick={() => selected !== null && playFrom(Math.max(0, splits[selected] - 4), splits[selected] + 4)}>
            <Icon name="play" /> 区切りの前後を聴く
          </button>
          <div class="spacer"></div>
          <button class="btn" onclick={removeSplit}><Icon name="trash" /> この区切りを消す</button>
        </div>
      </div>
    {:else}
      <p class="hint muted">
        波の絵を押すと、その場所から聴けます。赤い線（▼）が曲の区切りです。押すと、位置を直したり消したりできます。
      </p>
    {/if}
  </div>

  {#if message}
    <div class="notice"><Icon name="alert" /><span>{message}</span></div>
  {/if}

  <div class="panel">
    <label class="album-title">
      <span>アルバムの名前</span>
      <input
        class="input"
        value={album.albumTitle}
        oninput={(e) => onAlbumTitle(e.currentTarget.value)}
        placeholder="なくても大丈夫です"
      />
    </label>
    <ol class="tracks">
      {#each album.tracks as track, i (i)}
        <li class:current={playingTrack === i}>
          <span class="num">{i + 1}</span>
          <span class="side">{album.sides[track.side]?.label}面</span>
          <input
            class="input"
            value={titles[i] ?? ""}
            oninput={(e) => onTitle(i, e.currentTarget.value)}
            placeholder="{i + 1}曲目（名前はなくても大丈夫です）"
            aria-label="{i + 1}曲目の名前"
          />
          <span class="len">{formatDuration(track.end - track.start)}</span>
          {#if playingTrack === i}
            <button class="btn" onclick={pause}><Icon name="stop" /> 止める</button>
          {:else}
            <button class="btn" onclick={() => playFrom(track.start, track.end, i)}><Icon name="play" /> 聴く</button>
          {/if}
        </li>
      {/each}
    </ol>
  </div>

</div>

<style>
  .split-editor {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .panel {
    padding: 1.1rem 1.2rem;
    border-radius: 1rem;
    background: var(--surface-3);
    border: 1px solid var(--line);
  }
  .wave-card {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }
  .flipx {
    display: inline-flex;
    transform: scaleX(-1);
  }
  .position {
    font-variant-numeric: tabular-nums;
  }
  .selected-box {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0.8rem 1rem;
    border: 3px solid var(--focus);
    border-radius: 0.8rem;
  }
  .hint {
    font-size: 0.95rem;
  }
  .album-title {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin-bottom: 1rem;
    font-weight: 700;
  }
  .album-title span {
    white-space: nowrap;
  }
  .tracks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .tracks li {
    display: grid;
    grid-template-columns: 2.4rem 3rem 1fr 6.5rem auto;
    align-items: center;
    gap: 0.7rem;
    padding: 0.35rem 0.5rem;
    border-radius: 0.6rem;
  }
  .tracks li.current {
    background: var(--success-soft);
  }
  .num {
    display: grid;
    place-items: center;
    width: 2.2rem;
    height: 2.2rem;
    border-radius: 50%;
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 800;
  }
  .side {
    color: var(--text-muted);
    font-size: 0.9rem;
  }
  .len {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
