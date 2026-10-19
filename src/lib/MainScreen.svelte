<script lang="ts">
  import { app, scrollToSection } from "./state.svelte";
  import Icon from "./components/Icon.svelte";
  import Logo from "./components/Logo.svelte";
  import ProgressOverlay from "./components/ProgressOverlay.svelte";
  import RecordSection from "./RecordSection.svelte";
  import SplitSection from "./SplitSection.svelte";
  import CdSection from "./CdSection.svelte";

  const jumps = [
    { id: "record", n: 1, label: "録音する" },
    { id: "split", n: 2, label: "曲を分ける・保存" },
    { id: "cd", n: 3, label: "CDを作る" },
  ];
</script>

<div class="screen">
  <header class="top">
    <div class="brand">
      <Logo />
      <div>
        <h1>レコード録音</h1>
        <p class="tagline">録音 ・ 曲分け ・ CD作成</p>
      </div>
    </div>
    <nav class="jumps" aria-label="区画へ移動">
      {#each jumps as j (j.id)}
        <button class="jump" onclick={() => scrollToSection(j.id)}>
          <span class="jn">{j.n}</span>{j.label}
        </button>
      {/each}
    </nav>
    <button
      class="btn settings"
      disabled={app.busy}
      title={app.busy ? "作業中は設定を変えられません" : undefined}
      onclick={() => (app.screen = "settings")}
    >
      <Icon name="gear" /> 設定
    </button>
  </header>

  <main>
    <p class="hint">
      <Icon name="info" /> ①〜③は、それぞれ別々に使えます。下へスクロールすると、次の作業があります。
    </p>
    <RecordSection />
    <SplitSection />
    <CdSection />
    <footer class="foot muted">わからないときは、上から順番に進めてください。</footer>
  </main>
</div>

<ProgressOverlay />

<style>
  .screen {
    min-height: 100vh;
  }
  .top {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 1.2rem;
    padding: 0.7rem 1.5rem;
    background: var(--header-bg);
    color: var(--header-text);
    box-shadow: 0 2px 14px rgba(0, 0, 0, 0.25);
    flex-wrap: wrap;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  h1 {
    font-size: 1.3rem;
    font-weight: 900;
    letter-spacing: 0.06em;
  }
  .tagline {
    font-size: 0.75rem;
    opacity: 0.75;
    letter-spacing: 0.1em;
  }
  .jumps {
    flex: 1;
    display: flex;
    justify-content: center;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .jump {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.3rem 0.9rem 0.3rem 0.35rem;
    border: 1px solid rgba(255, 255, 255, 0.22);
    border-radius: 2rem;
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
    font: inherit;
    font-size: 0.95rem;
    font-weight: 700;
    cursor: pointer;
  }
  .jump:hover {
    background: rgba(255, 255, 255, 0.16);
  }
  .jn {
    display: grid;
    place-items: center;
    width: 1.6rem;
    height: 1.6rem;
    border-radius: 50%;
    background: #fff;
    color: #141925;
    font-weight: 900;
    font-size: 0.9rem;
  }
  .settings {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.45);
    color: #fff;
  }
  .settings:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.2);
  }
  main {
    max-width: 76rem;
    margin: 0 auto;
    padding: 1.4rem 1.4rem 3rem;
    display: flex;
    flex-direction: column;
    gap: 1.6rem;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--text-muted);
  }
  .foot {
    text-align: center;
    padding-top: 0.5rem;
  }
</style>
