<script lang="ts">
  import { store } from './store.svelte';
  import type { ActionResult, PlannedAction } from './types';
  import { CATEGORY_LABELS } from './types';

  let { onclose }: { onclose: () => void } = $props();

  let phase = $state<'loading' | 'preview' | 'applying' | 'results'>('loading');
  let plan = $state<PlannedAction[]>([]);
  let checked = $state<Set<number>>(new Set());
  let results = $state<ActionResult[]>([]);

  const KIND_LABELS: Record<string, string> = {
    Kill: 'End task',
    Suspend: 'Suspend',
    LowerPriority: 'Lower priority',
    TrimRam: 'Trim RAM',
  };

  $effect(() => {
    store.planOptimize().then((p) => {
      plan = p;
      checked = new Set(p.map((a) => a.pid));
      phase = 'preview';
    });
  });

  function toggle(pid: number) {
    const next = new Set(checked);
    if (next.has(pid)) next.delete(pid);
    else next.add(pid);
    checked = next;
  }

  async function apply() {
    phase = 'applying';
    const selected = plan.filter((a) => checked.has(a.pid));
    results = await store.applyOptimize(selected);
    phase = 'results';
  }

  const okCount = $derived(results.filter((r) => r.ok).length);
</script>

<div class="overlay" onclick={onclose} role="presentation">
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1" onkeydown={() => {}}>
    <h3>Optimize system</h3>

    {#if phase === 'loading'}
      <p class="muted">Analyzing running processes…</p>
    {:else if phase === 'preview'}
      {#if plan.length === 0}
        <p class="muted">Nothing to optimize - no resource hogs or non-essential helpers found. 🎉</p>
        <div class="buttons"><button class="btn" onclick={onclose}>Close</button></div>
      {:else}
        <p class="muted">
          {plan.length} suggested action{plan.length === 1 ? '' : 's'}. Uncheck anything you want to keep as-is -
          nothing happens until you hit Apply.
        </p>
        <div class="list">
          {#each plan as a (a.pid)}
            <label class="row">
              <input type="checkbox" checked={checked.has(a.pid)} onchange={() => toggle(a.pid)} />
              <span class="kind {a.kind}">{KIND_LABELS[a.kind]}</span>
              <span class="pname">{a.name} <span class="pid">({a.pid} · {CATEGORY_LABELS[a.category]})</span></span>
              <span class="reason">{a.reason}</span>
            </label>
          {/each}
        </div>
        <div class="buttons">
          <button class="btn" onclick={onclose}>Cancel</button>
          <button class="btn primary" disabled={checked.size === 0} onclick={apply}>
            Apply {checked.size} action{checked.size === 1 ? '' : 's'}
          </button>
        </div>
      {/if}
    {:else if phase === 'applying'}
      <p class="muted">Applying…</p>
    {:else}
      <p class="muted">{okCount}/{results.length} actions succeeded.</p>
      <div class="list">
        {#each results as r (r.pid + r.kind)}
          <div class="row result">
            <span class={r.ok ? 'ok' : 'fail'}>{r.ok ? '✓' : '✗'}</span>
            <span class="kind {r.kind}">{KIND_LABELS[r.kind]}</span>
            <span class="pname">{r.name} <span class="pid">({r.pid})</span></span>
            {#if r.error}<span class="reason fail">{r.error}</span>{/if}
          </div>
        {/each}
      </div>
      <div class="buttons"><button class="btn primary" onclick={onclose}>Done</button></div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 1.2rem 1.4rem;
    width: min(680px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
  }
  h3 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  .muted {
    color: var(--fg-dim);
    font-size: 0.85rem;
    margin: 0 0 0.8rem;
  }
  .list {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 1rem;
  }
  .row {
    display: grid;
    grid-template-columns: auto auto 1fr;
    grid-template-areas: 'check kind name' '. . reason';
    align-items: center;
    gap: 0.2rem 0.6rem;
    padding: 0.4rem 0.5rem;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.83rem;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.result {
    cursor: default;
  }
  .row input { grid-area: check; }
  .kind {
    grid-area: kind;
    font-size: 0.68rem;
    padding: 0.1rem 0.45rem;
    border-radius: 99px;
    white-space: nowrap;
  }
  .kind.Kill { background: var(--danger-bg); color: var(--danger); }
  .kind.Suspend { background: var(--warn-bg); color: var(--warn); }
  .kind.LowerPriority { background: var(--accent-bg); color: var(--accent); }
  .kind.TrimRam { background: var(--ok-bg); color: var(--ok); }
  .pname { grid-area: name; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pid { color: var(--fg-dim); font-size: 0.75rem; }
  .reason {
    grid-area: reason;
    color: var(--fg-dim);
    font-size: 0.74rem;
  }
  .ok { color: var(--ok); }
  .fail { color: var(--danger); }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
  }
</style>
