<script lang="ts">
  import type { Album } from "../api";
  import { formatClock } from "../api";

  let {
    album,
    splits,
    playhead,
    selected,
    onSeek,
    onSelect,
    onMove,
    onMoveEnd,
  }: {
    album: Album;
    /** 動かせる区切り（アルバム全体の秒） */
    splits: number[];
    playhead: number;
    selected: number | null;
    onSeek: (t: number) => void;
    onSelect: (index: number) => void;
    onMove: (index: number, t: number) => void;
    onMoveEnd: () => void;
  } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();
  let width = $state(0);
  let height = $state(0);

  const total = $derived(
    album.sides.length ? album.sides[album.sides.length - 1].offset + album.sides[album.sides.length - 1].duration : 1,
  );
  const pct = (t: number) => `${(t / total) * 100}%`;

  function readColor(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
  }

  // 画面の明るさが変わったら描き直す
  let themeTick = $state(0);
  $effect(() => {
    const observer = new MutationObserver(() => themeTick++);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => themeTick++;
    media.addEventListener("change", onChange);
    return () => {
      observer.disconnect();
      media.removeEventListener("change", onChange);
    };
  });

  $effect(() => {
    void themeTick;
    if (!canvas || width === 0 || height === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);
    const colors = [readColor("--wave"), readColor("--wave-alt")];
    const mid = height / 2;
    const secsPerPx = total / width;
    album.sides.forEach((side, si) => {
      ctx.fillStyle = colors[si % 2];
      const x0 = Math.floor((side.offset / total) * width);
      const x1 = Math.ceil(((side.offset + side.duration) / total) * width);
      for (let x = x0; x < x1; x++) {
        const from = Math.floor((x * secsPerPx - side.offset) / side.blockSecs);
        const to = Math.max(from + 1, Math.floor(((x + 1) * secsPerPx - side.offset) / side.blockSecs));
        let peak = 0;
        for (let b = Math.max(0, from); b < Math.min(side.peaks.length, to); b++) {
          if (side.peaks[b] > peak) peak = side.peaks[b];
        }
        const h = (peak / 255) * (mid - 4);
        ctx.fillRect(x, mid - h, 1, Math.max(1, h * 2));
      }
    });
  });

  function timeAt(clientX: number) {
    if (!box) return 0;
    const rect = box.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    return ratio * total;
  }

  // 区切りの線はドラッグでも動かせる（ボタンでも直せるので、ドラッグできなくても困らない）
  let dragging: number | null = null;
  let moved = false;
  function onHandleDown(e: PointerEvent, index: number) {
    e.stopPropagation();
    dragging = index;
    moved = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSelect(index);
  }
  function onHandleMove(e: PointerEvent) {
    if (dragging === null) return;
    moved = true;
    onMove(dragging, timeAt(e.clientX));
  }
  function onHandleUp() {
    if (dragging !== null && moved) onMoveEnd();
    dragging = null;
  }
</script>

<div class="wave-wrap">
  <div
    class="wave"
    bind:this={box}
    bind:clientWidth={width}
    bind:clientHeight={height}
    role="slider"
    tabindex="0"
    aria-label="録音した音の全体。押した所から再生します"
    aria-valuemin={0}
    aria-valuemax={Math.round(total)}
    aria-valuenow={Math.round(playhead)}
    aria-valuetext={formatClock(playhead)}
    onpointerdown={(e) => onSeek(timeAt(e.clientX))}
    onkeydown={(e) => {
      if (e.key === "ArrowLeft") onSeek(Math.max(0, playhead - 5));
      if (e.key === "ArrowRight") onSeek(Math.min(total, playhead + 5));
    }}
  >
    {#each album.tracks as track, i (i)}
      <div
        class="band"
        class:alt={i % 2 === 1}
        style:left={pct(track.start)}
        style:width={pct(track.end - track.start)}
      >
        <span class="track-num">{i + 1}</span>
      </div>
    {/each}
    <canvas bind:this={canvas}></canvas>
    {#each album.sides as side, si (side.label)}
      {#if si > 0}
        <div class="side-line" style:left={pct(side.offset)}></div>
      {/if}
      <span class="side-label" style:left={pct(side.offset)}>{side.label}面</span>
    {/each}
    <div class="playhead" style:left={pct(playhead)}></div>
    {#each splits as split, i (i)}
      <button
        class="split"
        class:selected={selected === i}
        style:left={pct(split)}
        aria-label="{i + 1}つめの区切り（{formatClock(split)}）"
        aria-pressed={selected === i}
        onpointerdown={(e) => onHandleDown(e, i)}
        onpointermove={onHandleMove}
        onpointerup={onHandleUp}
        onclick={(e) => {
          e.stopPropagation();
          if (!moved) onSelect(i);
        }}
      >
        <span class="handle" aria-hidden="true">▼</span>
      </button>
    {/each}
  </div>
  <div class="scale" aria-hidden="true">
    <span>0:00</span>
    <span>{formatClock(total / 2)}</span>
    <span>{formatClock(total)}</span>
  </div>
</div>

<style>
  .wave-wrap {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .wave {
    position: relative;
    height: 11rem;
    border-radius: 0.8rem;
    border: 2px solid var(--border);
    background: var(--surface);
    cursor: pointer;
    overflow: hidden;
    touch-action: none;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .band {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--track-band);
    pointer-events: none;
  }
  .band.alt {
    background: var(--track-band-alt);
  }
  .track-num {
    position: absolute;
    bottom: 0.2rem;
    left: 0.35rem;
    font-size: 0.85rem;
    font-weight: 800;
    color: var(--text-muted);
  }
  .side-line {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 4px dashed var(--text-muted);
    pointer-events: none;
  }
  .side-label {
    position: absolute;
    top: 0.2rem;
    margin-left: 0.4rem;
    font-size: 0.85rem;
    font-weight: 800;
    color: var(--text);
    background: var(--surface);
    padding: 0 0.4rem;
    border-radius: 0.3rem;
    pointer-events: none;
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    margin-left: -1px;
    background: var(--playhead);
    pointer-events: none;
  }
  .split {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2.2rem;
    margin-left: -1.1rem;
    padding: 0;
    border: none;
    background: transparent;
    cursor: ew-resize;
    touch-action: none;
  }
  .split::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 4px;
    margin-left: -2px;
    background: var(--split);
  }
  .split .handle {
    position: absolute;
    top: -0.15rem;
    left: 50%;
    transform: translateX(-50%);
    font-size: 1.2rem;
    color: var(--split);
    z-index: 1;
  }
  .split.selected::after {
    width: 8px;
    margin-left: -4px;
    background: var(--focus);
  }
  .split.selected .handle {
    color: var(--focus);
    font-size: 1.5rem;
  }
  .scale {
    display: flex;
    justify-content: space-between;
    font-size: 0.8rem;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
