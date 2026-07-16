<script lang="ts">
  import Sparkline from './Sparkline.svelte';
  import type { Snippet } from 'svelte';

  let {
    title,
    value,
    sub = '',
    history = [] as number[],
    max = 100,
    color = '#4f9cf9',
    disabled = false,
    disabledReason = '',
    onclick = undefined as (() => void) | undefined,
    extra = undefined as Snippet | undefined,
  } = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="card"
  class:disabled
  class:clickable={!!onclick}
  onclick={onclick}
  onkeydown={(e) => e.key === 'Enter' && onclick?.()}
  role={onclick ? 'button' : 'group'}
  tabindex={onclick ? 0 : -1}
>
  <div class="head">
    <span class="title">{title}</span>
    <span class="value" style="color: {disabled ? 'var(--fg-dim)' : color}">{value}</span>
  </div>
  {#if disabled}
    <div class="reason">{disabledReason}</div>
  {:else}
    <Sparkline data={history} {max} {color} />
    <div class="sub">{sub}</div>
    {#if extra}{@render extra()}{/if}
  {/if}
</div>

<style>
  .card {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 0.7rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    min-width: 0;
  }
  .card.clickable {
    cursor: pointer;
  }
  .card.clickable:hover {
    border-color: var(--accent);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.5rem;
  }
  .title {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-dim);
  }
  .value {
    font-size: 1.25rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .sub {
    font-size: 0.72rem;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .reason {
    font-size: 0.75rem;
    color: var(--fg-dim);
    padding: 0.5rem 0;
  }
</style>
