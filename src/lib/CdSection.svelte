<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, CD_GAP, CD_MAX, errorText, formatDuration, type AudioFile, type DiscStatus } from "./api";
  import { app } from "./state.svelte";
  import Section from "./components/Section.svelte";
  import Icon from "./components/Icon.svelte";
  import CdCapacity from "./components/CdCapacity.svelte";

  type Source = "album" | "files";
  let chosen = $state<Source | null>(null);
  let files = $state<AudioFile[]>([]);
  let adding = $state(false);

  let disc = $state<DiscStatus | null>(null);
  let checking = $state(false);
  let burnDone = $state(false);
  let error = $state<string | null>(null);

  const album = $derived(app.album);
  // 選んでいなければ、曲分けした曲があればそれを使う
  const source = $derived<Source>(chosen ?? (album ? "album" : "files"));
  const usable = $derived(files.filter((f) => f.duration !== null));
  const trackCount = $derived(source === "album" ? (album?.tracks.length ?? 0) : usable.length);
  const seconds = $derived(
    source === "album"
      ? (album?.cdSeconds ?? 0)
      : usable.reduce((sum, f) => sum + (f.duration ?? 0), 0) +
          Math.max(0, usable.length - 1) * CD_GAP,
  );
  const fits = $derived(seconds <= CD_MAX);
  const canBurn = $derived(
    disc?.status === "ready" && fits && trackCount > 0 && trackCount <= 99 && !app.busy,
  );

  async function check() {
    if (checking || app.task) return;
    checking = true;
    try {
      disc = await api.checkDisc();
    } catch (e) {
      disc = { status: "unknown", message: errorText(e) };
    }
    checking = false;
  }

  // CDを入れたら自然に「準備ができました」に変わるよう、数秒おきにそっと確かめる
  let timer = 0;
  onMount(() => {
    check();
    timer = window.setInterval(() => {
      if (!app.busy && !burnDone) check();
    }, 5000);
  });
  onDestroy(() => clearInterval(timer));

  function discMessage(d: DiscStatus | null): { text: string; ok: boolean } {
    if (!d) return { text: "CDドライブを確かめています…", ok: false };
    switch (d.status) {
      case "ready":
        return { text: "新しいCDが入っています。CDを作れます。", ok: true };
      case "noDisc":
        return { text: "新しいCD-R（何も書かれていないCD）を入れてください。", ok: false };
      case "notBlank":
        return { text: "このCDには、すでに何か書かれています。新しいCD-Rを入れてください。", ok: false };
      case "wrongMedia":
        return { text: "これはCDではありません（DVDなど）。CD-Rを入れてください。", ok: false };
      case "noDrive":
        return { text: "CDドライブが見つかりません。CDドライブをパソコンにつないでください。", ok: false };
      case "noTool":
        return {
          text: "CDを書き込むためのソフト（cdrskin）が入っていません。アプリを入れ直すか、cdrskin を入れてください。",
          ok: false,
        };
      default:
        return { text: d.message, ok: false };
    }
  }
  const discInfo = $derived(discMessage(disc));

  async function addFiles() {
    const paths = await app.pickAudio(true);
    if (paths.length === 0) return;
    adding = true;
    try {
      const fresh = paths.filter((p) => !files.some((f) => f.path === p));
      files = [...files, ...(await api.probeFiles(fresh))];
      chosen = "files";
    } finally {
      adding = false;
    }
  }

  function move(i: number, delta: number) {
    const j = i + delta;
    if (j < 0 || j >= files.length) return;
    const next = [...files];
    [next[i], next[j]] = [next[j], next[i]];
    files = next;
  }

  function remove(i: number) {
    files = files.filter((_, k) => k !== i);
  }

  async function burn() {
    const ok = await app.confirm({
      title: "音楽CDを作りますか？",
      message: `${trackCount}曲（${formatDuration(seconds)}）をCDに書き込みます。\n10分ほどかかります。終わるまで、CDを取り出したり、パソコンを閉じたりしないでください。`,
      ok: "CDを作りはじめる",
      cancel: "やめる",
    });
    if (!ok) return;
    error = null;
    burnDone = false;
    try {
      await app.run("burn", () =>
        source === "album" ? api.burnCd() : api.burnFiles(usable.map((f) => f.path)),
      );
      burnDone = true;
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<Section
  id="cd"
  number={3}
  icon="disc"
  tone="teal"
  title="音楽CDを作る"
  lead="CDプレーヤーや車で聴ける、ふつうの音楽CDを作ります（80分まで）。"
  status={burnDone ? "CDができました" : undefined}
>
  <div class="choices" role="radiogroup" aria-label="CDに入れる曲">
    <button
      class="choice"
      class:on={source === "album"}
      role="radio"
      aria-checked={source === "album"}
      disabled={!album || app.busy}
      onclick={() => (chosen = "album")}
    >
      <span class="radio" aria-hidden="true"></span>
      <span class="choice-text">
        <strong>「② 曲を分ける」の曲</strong>
        <span class="muted">
          {#if album}{album.tracks.length}曲・{formatDuration(album.cdSeconds)}{:else}まだ曲がありません{/if}
        </span>
      </span>
    </button>
    <button
      class="choice"
      class:on={source === "files"}
      role="radio"
      aria-checked={source === "files"}
      disabled={app.busy}
      onclick={() => (chosen = "files")}
    >
      <span class="radio" aria-hidden="true"></span>
      <span class="choice-text">
        <strong>パソコンにある曲を選ぶ</strong>
        <span class="muted">MP3・WAV などのファイルから作ります</span>
      </span>
    </button>
  </div>

  {#if source === "files"}
    <div class="files">
      {#if files.length === 0}
        <p class="muted">「曲を追加する」を押して、CDに入れる曲を選んでください。上から順にCDに入ります。</p>
      {:else}
        <ol class="file-list">
          {#each files as file, i (file.path)}
            <li class:bad={file.duration === null}>
              <span class="num">{i + 1}</span>
              <span class="name">
                {file.name}
                {#if file.error}<span class="bad-text">（読み込めないので入りません）</span>{/if}
              </span>
              <span class="len">{file.duration !== null ? formatDuration(file.duration) : "—"}</span>
              <span class="tools">
                <button class="btn btn-small" disabled={i === 0 || app.busy} onclick={() => move(i, -1)} aria-label="{i + 1}曲目を上へ">
                  <Icon name="up" /> 上へ
                </button>
                <button class="btn btn-small" disabled={i === files.length - 1 || app.busy} onclick={() => move(i, 1)} aria-label="{i + 1}曲目を下へ">
                  <Icon name="down" /> 下へ
                </button>
                <button class="btn btn-small" disabled={app.busy} onclick={() => remove(i)} aria-label="{i + 1}曲目を外す">
                  <Icon name="trash" /> 外す
                </button>
              </span>
            </li>
          {/each}
        </ol>
      {/if}
      <div class="row">
        <button class="btn" disabled={adding || app.busy} onclick={addFiles}>
          <Icon name="add" /> {adding ? "読み込んでいます…" : "曲を追加する"}
        </button>
        {#if files.length > 0}
          <button class="btn btn-small" disabled={app.busy} onclick={() => (files = [])}>すべて外す</button>
        {/if}
      </div>
    </div>
  {/if}

  <CdCapacity {seconds} max={CD_MAX} />

  <div class="burn">
    <div class="disc-state" class:ok={discInfo.ok && !burnDone}>
      {#if burnDone}
        <Icon name="check" size="1.6rem" />
        <span>CDができあがりました！ CDが出てきます。もう1枚作るときは、新しいCD-Rを入れてください。</span>
      {:else}
        <Icon name={discInfo.ok ? "check" : "disc"} size="1.6rem" />
        <span>{discInfo.text}</span>
      {/if}
    </div>
    <div class="row">
      <button class="btn btn-big btn-teal" disabled={!canBurn} onclick={burn}>
        <Icon name="disc" /> CDを作る
      </button>
      <button class="btn" disabled={checking || app.busy} onclick={() => { burnDone = false; check(); }}>
        {checking ? "確かめています…" : "CDをもう一度確かめる"}
      </button>
    </div>
    {#if trackCount === 0}
      <p class="muted">CDに入れる曲がまだありません。</p>
    {/if}
  </div>

  {#if error}
    <div class="notice error"><Icon name="alert" /><span>{error}</span></div>
  {/if}
</Section>

<style>
  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.9rem;
  }
  @media (max-width: 860px) {
    .choices {
      grid-template-columns: 1fr;
    }
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 1rem 1.2rem;
    border: 2px solid var(--line);
    border-radius: 1rem;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .choice:hover:not(:disabled) {
    border-color: var(--border);
  }
  .choice:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .choice.on {
    border-color: var(--teal);
    border-width: 3px;
    background: var(--teal-soft);
  }
  .radio {
    flex: none;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 50%;
    border: 3px solid var(--border);
  }
  .choice.on .radio {
    border-color: var(--teal);
    background: radial-gradient(circle, var(--teal) 0 45%, transparent 50%);
  }
  .choice-text {
    display: flex;
    flex-direction: column;
  }
  .files {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    padding: 1.1rem 1.2rem;
    border-radius: 1rem;
    background: var(--surface-3);
    border: 1px solid var(--line);
  }
  .file-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .file-list li {
    display: grid;
    grid-template-columns: 2.2rem 1fr auto auto;
    align-items: center;
    gap: 0.8rem;
    padding: 0.4rem 0.6rem;
    border-radius: 0.7rem;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .file-list li.bad {
    opacity: 0.7;
  }
  .num {
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    background: var(--teal);
    color: var(--on-primary);
    font-weight: 800;
  }
  .name {
    font-weight: 700;
    word-break: break-all;
  }
  .bad-text {
    color: var(--error-text);
    font-weight: 400;
  }
  .len {
    font-variant-numeric: tabular-nums;
  }
  .tools {
    display: flex;
    gap: 0.4rem;
  }
  .burn {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    padding: 1.2rem 1.3rem;
    border-radius: 1rem;
    background: var(--teal-soft);
  }
  .disc-state {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    font-size: 1.1rem;
    font-weight: 700;
  }
  .disc-state.ok {
    color: var(--success);
  }
</style>
