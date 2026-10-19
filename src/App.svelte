<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/state.svelte";
  import MainScreen from "./lib/MainScreen.svelte";
  import SettingsScreen from "./lib/SettingsScreen.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";

  let ready = $state(false);
  let startupError = $state<string | null>(null);

  onMount(async () => {
    try {
      await app.init();
    } catch (e) {
      startupError = String(e);
    }
    ready = true;
  });

  // 画面の明るさと文字の大きさを反映する
  $effect(() => {
    const root = document.documentElement;
    const theme = app.settings?.theme ?? "auto";
    if (theme === "auto") delete root.dataset.theme;
    else root.dataset.theme = theme;
    root.dataset.font = app.settings?.fontSize ?? "large";
  });
</script>

{#if !ready}
  <div class="loading">準備しています…</div>
{:else if startupError}
  <div class="loading">
    <div class="notice error">アプリを始められませんでした。いったん閉じて、もう一度開いてください。（{startupError}）</div>
  </div>
{:else if app.screen === "settings"}
  <SettingsScreen />
{:else}
  <MainScreen />
{/if}

{#if app.dialog}
  <ConfirmDialog request={app.dialog} />
{/if}

<style>
  .loading {
    display: grid;
    place-items: center;
    min-height: 100vh;
    font-size: 1.3rem;
    padding: 2rem;
  }
</style>
