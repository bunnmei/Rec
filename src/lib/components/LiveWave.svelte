<script lang="ts">
  import { formatClock } from "../api";

  /** 0.05秒ごとの音の大きさ（0〜255）。tick が変わったら描き直す */
  let { peaks, tick, recording }: { peaks: number[]; tick: number; recording: boolean } = $props();

  const BLOCK = 0.05;
  /** 横幅いっぱいで最低何秒ぶん見せるか（短いうちに波形が引き伸ばされないように） */
  const MIN_SPAN = 120;

  let canvas: HTMLCanvasElement | undefined = $state();
  let width = $state(0);
  let height = $state(0);

  const span = $derived.by(() => {
    void tick;
    return Math.max(MIN_SPAN, peaks.length * BLOCK * 1.1);
  });

  const empty = $derived.by(() => {
    void tick;
    return peaks.length === 0;
  });

  function color(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  $effect(() => {
    void tick;
    if (!canvas || width === 0 || height === 0) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr)) canvas.width = Math.round(width * dpr);
    if (canvas.height !== Math.round(height * dpr)) canvas.height = Math.round(height * dpr);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    // 1分ごとの目盛り
    ctx.strokeStyle = color("--display-grid");
    ctx.lineWidth = 1;
    for (let t = 60; t < span; t += 60) {
      const x = Math.round((t / span) * width) + 0.5;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
    }
    const mid = height / 2;
    ctx.beginPath();
    ctx.moveTo(0, mid + 0.5);
    ctx.lineTo(width, mid + 0.5);
    ctx.stroke();

    // 波形
    const perPx = span / BLOCK / width;
    const drawn = Math.min(width, Math.ceil(peaks.length / perPx));
    const grad = ctx.createLinearGradient(0, 0, 0, height);
    const wave = color("--display-wave");
    grad.addColorStop(0, wave);
    grad.addColorStop(0.5, "#b8e6ff");
    grad.addColorStop(1, wave);
    ctx.fillStyle = grad;
    for (let x = 0; x < drawn; x++) {
      const from = Math.floor(x * perPx);
      const to = Math.max(from + 1, Math.floor((x + 1) * perPx));
      let peak = 0;
      for (let i = from; i < Math.min(peaks.length, to); i++) if (peaks[i] > peak) peak = peaks[i];
      const h = (peak / 255) * (mid - 3);
      ctx.fillRect(x, mid - h, 1, Math.max(1, h * 2));
    }
    // いま録音している位置
    if (recording && drawn > 0) {
      ctx.fillStyle = color("--display-rec");
      ctx.fillRect(drawn - 1, 0, 2, height);
    }
  });
</script>

<div class="live">
  <div class="screen" bind:clientWidth={width} bind:clientHeight={height}>
    <canvas bind:this={canvas}></canvas>
    {#if empty}
      <div class="empty">音が聞こえると、ここに波の形が出ます</div>
    {/if}
  </div>
  <div class="scale" aria-hidden="true">
    <span>0:00</span>
    <span>{formatClock(span / 2)}</span>
    <span>{formatClock(span)}</span>
  </div>
</div>

<style>
  .live {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .screen {
    position: relative;
    height: 9rem;
    border-radius: 0.8rem;
    background: color-mix(in srgb, var(--display-bg) 70%, black);
    border: 1px solid var(--display-grid);
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--display-muted);
    font-size: 0.95rem;
  }
  .scale {
    display: flex;
    justify-content: space-between;
    font-size: 0.8rem;
    color: var(--display-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
