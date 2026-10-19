<script lang="ts">
  let { left, right }: { left: number; right: number } = $props();

  // 小さな音も見えるよう、-60dB〜0dB を 0〜100% に
  function percent(v: number) {
    const db = 20 * Math.log10(Math.max(v, 1e-6));
    return Math.min(100, Math.max(0, ((db + 60) / 60) * 100));
  }
</script>

<div class="meter" aria-label="音の大きさ">
  {#each [{ name: "左", v: left }, { name: "右", v: right }] as ch (ch.name)}
    <div class="channel">
      <span class="name">{ch.name}</span>
      <div class="track">
        <div class="fill" style:clip-path="inset(0 {100 - percent(ch.v)}% 0 0)"></div>
      </div>
    </div>
  {/each}
</div>

<style>
  .meter {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    width: 100%;
  }
  .channel {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .name {
    width: 1.4rem;
    font-weight: 800;
    color: var(--display-muted);
    font-size: 0.9rem;
  }
  .track {
    flex: 1;
    height: 0.9rem;
    border-radius: 0.45rem;
    background: var(--display-grid);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    /* 緑 → 黄 → 赤（大きすぎ） */
    background: linear-gradient(90deg, #2fbf71 0%, #2fbf71 70%, #f2c037 85%, #ff5a4f 100%);
    transition: clip-path 0.08s linear;
  }
</style>
