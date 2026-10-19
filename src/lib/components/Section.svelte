<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    id,
    number,
    title,
    lead,
    icon,
    tone,
    status,
    children,
  }: {
    id: string;
    number: number;
    title: string;
    lead: string;
    icon: IconName;
    tone: "record" | "primary" | "teal";
    /** 右上に出す短い状態（「録音済み」など） */
    status?: string;
    children: Snippet;
  } = $props();
</script>

<section {id} class="section tone-{tone}" aria-labelledby="{id}-title">
  <header class="head">
    <div class="medal" aria-hidden="true">
      <span class="num">{number}</span>
    </div>
    <div class="titles">
      <h2 id="{id}-title"><Icon name={icon} size="1em" /> {title}</h2>
      <p class="lead">{lead}</p>
    </div>
    {#if status}
      <span class="status"><Icon name="check" size="1em" /> {status}</span>
    {/if}
  </header>
  <div class="body">
    {@render children()}
  </div>
</section>

<style>
  .section {
    --tone: var(--primary);
    --tone-soft: var(--primary-soft);
    position: relative;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 1.4rem;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .tone-record {
    --tone: var(--record);
    --tone-soft: var(--record-soft);
  }
  .tone-teal {
    --tone: var(--teal);
    --tone-soft: var(--teal-soft);
  }
  .section::before {
    content: "";
    position: absolute;
    inset: 0 0 auto 0;
    height: 0.4rem;
    background: var(--tone);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 1.1rem;
    padding: 1.5rem 1.8rem 1.1rem;
    border-bottom: 1px solid var(--line);
    background: linear-gradient(180deg, var(--tone-soft), transparent);
  }
  .medal {
    flex: none;
    display: grid;
    place-items: center;
    width: 3.4rem;
    height: 3.4rem;
    border-radius: 50%;
    background: var(--tone);
    color: var(--on-primary);
    box-shadow: var(--shadow);
  }
  .num {
    font-size: 1.6rem;
    font-weight: 900;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 1.55rem;
  }
  h2 :global(.icon) {
    color: var(--tone);
  }
  .lead {
    color: var(--text-muted);
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.25rem 0.9rem;
    border-radius: 2rem;
    background: var(--success-soft);
    color: var(--success);
    font-weight: 800;
    white-space: nowrap;
  }
  .body {
    padding: 1.4rem 1.8rem 1.8rem;
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }
</style>
