<script lang="ts">
  import type { DialogRequest } from "../state.svelte";

  let { request }: { request: DialogRequest } = $props();
  let cancelButton: HTMLButtonElement | undefined = $state();

  // うっかり Enter を押しても危ない操作にならないよう、「やめる」側を選んだ状態で開く
  $effect(() => {
    cancelButton?.focus();
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") request.resolve(false);
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop" role="presentation">
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="dialog-title">
    <h2 id="dialog-title">{request.title}</h2>
    <p class="message">{request.message}</p>
    <div class="buttons">
      <button class="btn btn-big" bind:this={cancelButton} onclick={() => request.resolve(false)}>
        {request.cancel}
      </button>
      <button
        class="btn btn-big {request.danger ? 'btn-record' : 'btn-primary'}"
        onclick={() => request.resolve(true)}
      >
        {request.ok}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
    z-index: 100;
    padding: 1.5rem;
  }
  .dialog {
    max-width: 42rem;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 1.2rem;
    padding: 2rem;
  }
  h2 {
    font-size: 1.45rem;
  }
  .message {
    font-size: 1.1rem;
    white-space: pre-line;
  }
  .buttons {
    display: flex;
    gap: 1rem;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
</style>
