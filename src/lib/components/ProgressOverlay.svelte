<script lang="ts">
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const title = $derived(
    app.task === "burn"
      ? app.progress?.phase === "burn"
        ? "CDに書き込んでいます"
        : "CDに書き込む準備をしています"
      : "パソコンに保存しています",
  );
  const percent = $derived(Math.round((app.progress?.progress ?? 0) * 100));
</script>

{#if app.task}
  <div class="backdrop">
    <div class="box" role="status" aria-live="polite">
      <div class="spin" aria-hidden="true"><Icon name={app.task === "burn" ? "disc" : "save"} size="2.4rem" /></div>
      <h2>{title}</h2>
      <div class="bar"><div style:width="{percent}%"></div></div>
      <p class="pct">
        {percent}%
        {#if app.progress && app.progress.tracks > 0}
          <span class="muted">（{app.progress.track}曲目 / {app.progress.tracks}曲）</span>
        {/if}
      </p>
      {#if app.task === "burn"}
        <p class="caution">終わるまで、CDを取り出したり、パソコンを閉じたりしないでください。</p>
      {:else}
        <p class="muted">終わるまで、少しお待ちください。</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    padding: 1.5rem;
    background: rgba(8, 10, 16, 0.6);
    backdrop-filter: blur(3px);
  }
  .box {
    width: 100%;
    max-width: 40rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 1rem;
    padding: 2.2rem 2rem;
    border-radius: 1.4rem;
    background: var(--surface);
    box-shadow: var(--shadow-strong);
  }
  .spin {
    color: var(--teal);
    animation: spin 2.4s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  h2 {
    font-size: 1.5rem;
  }
  .pct {
    font-size: 1.6rem;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }
  .caution {
    font-weight: 800;
    color: var(--record);
  }
</style>
