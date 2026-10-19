<script lang="ts">
  import { openPath } from "@tauri-apps/plugin-opener";
  import { api, errorText, formatDuration } from "./api";
  import { app, scrollToSection } from "./state.svelte";
  import Section from "./components/Section.svelte";
  import Icon from "./components/Icon.svelte";
  import SplitEditor from "./SplitEditor.svelte";

  let error = $state<string | null>(null);
  let loading = $state(false);
  let savedDir = $state<string | null>(null);

  const album = $derived(app.album);
  const total = $derived(
    album ? album.tracks.reduce((sum, t) => sum + (t.end - t.start), 0) : 0,
  );
  const formats = $derived(app.settings?.saveMp3 ? "WAV と MP3" : "WAV");

  async function importFile() {
    if (album) {
      const ok = await app.confirm({
        title: "ファイルを読み込みますか？",
        message: album.imported
          ? "いま使っているファイルの曲の区切りは消えて、選んだファイルに入れかわります。"
          : "いま録音してある音と曲の区切りは消えて、選んだファイルに入れかわります。\n（録音した音を残したいときは、先に「録音したまま保存する」を押してください）",
        ok: "入れかえる",
        cancel: "やめる",
        danger: true,
      });
      if (!ok) return;
    }
    const path = await app.pickAudio(false);
    if (!path) return;
    error = null;
    savedDir = null;
    loading = true;
    try {
      app.album = await api.importAudio(path);
      app.lastFinished = null;
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  async function save() {
    error = null;
    savedDir = null;
    try {
      savedDir = await app.run("save", api.saveFiles);
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<Section
  id="split"
  number={2}
  icon="cut"
  tone="primary"
  title="曲を分けて保存する"
  lead="曲と曲のあいだを自動で見つけて分けます。曲ごとにパソコンへ保存できます。"
  status={album && !app.recording ? `${album.tracks.length}曲` : undefined}
>
  {#if app.recording}
    <div class="notice info"><Icon name="info" /><span>録音が終わると、ここに曲が並びます。</span></div>
  {:else if loading}
    <div class="notice info"><Icon name="info" /><span>ファイルを読み込んでいます。少しお待ちください…</span></div>
  {:else if !album}
    <div class="empty">
      <div class="empty-icon"><Icon name="music" size="3rem" /></div>
      <p class="empty-title">まだ曲がありません</p>
      <p class="muted">「① 録音する」でレコードを録音するか、パソコンにある音声ファイル（WAV・MP3 など）を読み込んでください。</p>
      <div class="row center">
        <button class="btn btn-big btn-primary" disabled={app.busy} onclick={importFile}>
          <Icon name="file" /> 音声ファイルを読み込む
        </button>
        <button class="btn btn-big" onclick={() => scrollToSection("record")}>
          <Icon name="record" /> 録音する
        </button>
      </div>
    </div>
  {:else}
    <div class="source row">
      <span>
        {#if album.imported}
          <Icon name="file" /> 読み込んだファイル：<strong>{album.albumTitle}</strong>
        {:else}
          <Icon name="record" /> 録音したレコード（{album.sides.map((s) => s.label + "面").join("・")}）
        {/if}
        <span class="muted">— {album.tracks.length}曲・{formatDuration(total)}</span>
      </span>
      <span class="spacer"></span>
      <button class="btn btn-small" disabled={app.busy} onclick={importFile}><Icon name="file" /> 別のファイルを読み込む</button>
    </div>

    <SplitEditor />

    <div class="save">
      <div class="save-text">
        <h3><Icon name="save" /> パソコンに保存する</h3>
        <p class="muted">曲ごとのファイル（{formats}）を保存します。スマートフォンや音楽ソフトで使えます。</p>
      </div>
      <button class="btn btn-big btn-primary" disabled={app.busy} onclick={save}>
        <Icon name="save" /> 曲ごとに保存する
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
    <div class="next">
      <button class="btn" onclick={() => scrollToSection("cd")}>
        <Icon name="arrowDown" /> ③ CDを作るへ
      </button>
    </div>
  {/if}

  {#if error}
    <div class="notice error"><Icon name="alert" /><span>{error}</span></div>
  {/if}
</Section>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 0.6rem;
    padding: 1.5rem 1rem;
    border: 2px dashed var(--line);
    border-radius: 1rem;
  }
  .empty-icon {
    color: var(--primary);
  }
  .empty-title {
    font-size: 1.3rem;
    font-weight: 800;
  }
  .center {
    justify-content: center;
    margin-top: 0.5rem;
  }
  .source {
    font-weight: 700;
  }
  .save {
    display: flex;
    align-items: center;
    gap: 1.2rem;
    flex-wrap: wrap;
    padding: 1.1rem 1.3rem;
    border-radius: 1rem;
    background: var(--primary-soft);
  }
  .save-text {
    flex: 1;
    min-width: 18rem;
  }
  .save h3 {
    font-size: 1.25rem;
  }
  .next {
    display: flex;
    justify-content: flex-end;
  }
</style>
