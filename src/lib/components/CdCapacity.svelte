<script lang="ts">
  import { formatDuration } from "../api";

  let { seconds, max }: { seconds: number; max: number } = $props();

  const over = $derived(seconds > max);
  const percent = $derived(Math.min(100, (seconds / max) * 100));
</script>

<div class="capacity">
  <div class="row">
    <span class="label">CDに入る長さ</span>
    <span>
      80分のうち <strong>{formatDuration(seconds)}</strong>
      {#if !over}
        <span class="muted">（あと {formatDuration(max - seconds)} 入ります）</span>
      {/if}
    </span>
  </div>
  <div class="bar" class:over role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
    <div style:width="{percent}%"></div>
  </div>
  {#if over}
    <div class="notice error">
      <span>CD1枚に入りません（{formatDuration(seconds - max)} 多すぎます）。曲を減らしてください。</span>
    </div>
  {/if}
</div>

<style>
  .capacity {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .label {
    font-weight: 800;
  }
</style>
