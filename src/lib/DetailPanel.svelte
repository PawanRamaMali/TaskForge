<script lang="ts">
  import { store } from './store.svelte';
  import { fmtBytes, fmtBps, fmtPct, fmtDuration, fmtTime } from './format';
  import {
    CATEGORY_LABELS,
    PRIORITY_LEVELS,
    PRIORITY_LABELS,
    type ProcInfo,
    type ProcessDetails,
    type PriorityLevel,
  } from './types';

  let { proc, onclose }: { proc: ProcInfo; onclose: () => void } = $props();

  let details = $state<ProcessDetails | null>(null);
  let affinity = $state<boolean[]>([]);
  let coreCount = $state(0);
  let rulePriority = $state<PriorityLevel>('BelowNormal');
  let ruleEfficiency = $state(false);

  // Reload details whenever the selected pid changes.
  $effect(() => {
    const pid = proc.pid;
    details = null;
    store.getProcessDetails(pid).then((d) => {
      if (!d || d.pid !== proc.pid) return;
      details = d;
      coreCount = d.coreCount;
      const mask = d.affinityMask ?? (coreCount >= 64 ? -1 : (1 << coreCount) - 1);
      affinity = Array.from({ length: coreCount }, (_, i) =>
        coreCount >= 53 ? true : (mask & (1 << i)) !== 0
      );
    });
  });

  const existingRule = $derived(
    store.rules.find((r) => r.exe === proc.name.toLowerCase()) ?? null
  );

  function toggleCore(i: number) {
    affinity[i] = !affinity[i];
    affinity = [...affinity];
  }

  function selectedMask(): number {
    let mask = 0;
    for (let i = 0; i < affinity.length; i++) if (affinity[i]) mask |= 1 << i;
    return mask >>> 0;
  }

  async function applyAffinity() {
    const mask = selectedMask();
    if (mask === 0) {
      store.toast('error', 'Select at least one core');
      return;
    }
    await store.setAffinity(proc.pid, mask, proc.name);
  }

  async function saveRule() {
    await store.setRule(proc.name, rulePriority, ruleEfficiency);
  }
</script>

<aside class="panel">
  <div class="head">
    <div class="title">
      <span class="pname">{proc.name}</span>
      <span class="pid">PID {proc.pid}</span>
    </div>
    <button class="close" onclick={onclose} title="Close">✕</button>
  </div>

  <div class="body">
    <section>
      <div class="badges">
        <span class="badge cat-{proc.category}">{CATEGORY_LABELS[proc.category]}</span>
        {#if proc.suspended}<span class="badge warn">suspended</span>{/if}
        {#if proc.safeToKill}<span class="badge acc">optimizable</span>{/if}
        <span class="badge">{proc.status}</span>
      </div>
    </section>

    <section class="metrics">
      <div><span>CPU</span><b>{fmtPct(proc.cpu)}</b></div>
      <div><span>Memory</span><b>{fmtBytes(proc.memBytes)}</b></div>
      <div><span>Disk</span><b>{fmtBps(proc.diskReadBps + proc.diskWriteBps)}</b></div>
      <div><span>GPU</span><b>{proc.gpuUtil != null ? fmtPct(proc.gpuUtil, 0) : '-'}</b></div>
      <div><span>Uptime</span><b>{fmtDuration(proc.runTime)}</b></div>
      <div><span>Priority</span><b>{details?.priority ?? '…'}</b></div>
    </section>

    <section class="kv">
      <div class="row"><span>User</span><code>{proc.user ?? '-'}</code></div>
      <div class="row"><span>Parent PID</span><code>{proc.parentPid ?? '-'}</code></div>
      <div class="row"><span>Started</span><code>{fmtTime(proc.startTime)}</code></div>
      <div class="row"><span>Path</span><code class="wrap">{proc.exe ?? '-'}</code></div>
      {#if details?.cwd}<div class="row"><span>Working dir</span><code class="wrap">{details.cwd}</code></div>{/if}
      {#if details && details.cmd.length}
        <div class="row"><span>Command</span><code class="wrap">{details.cmd.join(' ')}</code></div>
      {/if}
      {#if details && details.environCount > 0}
        <div class="row"><span>Env vars</span><code>{details.environCount}</code></div>
      {/if}
    </section>

    <section>
      <h4>CPU affinity {details ? `(${affinity.filter(Boolean).length}/${coreCount} cores)` : ''}</h4>
      {#if details && coreCount > 0 && coreCount < 64}
        <div class="cores">
          {#each affinity as on, i}
            <button class="core" class:on={on} onclick={() => toggleCore(i)} title="Core {i}">{i}</button>
          {/each}
        </div>
        <button class="btn small" onclick={applyAffinity}>Apply affinity</button>
      {:else}
        <p class="muted">Affinity editing unavailable for this process.</p>
      {/if}
    </section>

    <section>
      <h4>Persistent priority rule</h4>
      {#if existingRule}
        <p class="muted">
          Active rule: always <strong>{PRIORITY_LABELS[existingRule.priority]}</strong>{existingRule.efficiencyMode ? ' + efficiency mode' : ''}.
        </p>
        <button class="btn small danger" onclick={() => store.removeRule(proc.name)}>Remove rule</button>
      {:else}
        <p class="muted">Automatically apply a priority to <strong>{proc.name}</strong> every time it starts.</p>
        <div class="rule-form">
          <select bind:value={rulePriority}>
            {#each PRIORITY_LEVELS as lvl}
              <option value={lvl}>{PRIORITY_LABELS[lvl]}</option>
            {/each}
          </select>
          <label class="chk"><input type="checkbox" bind:checked={ruleEfficiency} /> Efficiency mode</label>
          <button class="btn small" onclick={saveRule}>Save rule</button>
        </div>
      {/if}
    </section>
  </div>
</aside>

<style>
  .panel {
    width: 340px;
    flex-shrink: 0;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7rem 0.9rem;
    border-bottom: 1px solid var(--border);
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }
  .pname {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pid {
    font-size: 0.72rem;
    color: var(--fg-dim);
  }
  .close {
    background: none;
    border: none;
    color: var(--fg-dim);
    cursor: pointer;
    font-size: 0.9rem;
    padding: 0.2rem 0.4rem;
    border-radius: 6px;
  }
  .close:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }
  .body {
    overflow-y: auto;
    padding: 0.8rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }
  section h4 {
    margin: 0 0 0.5rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-dim);
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .badge {
    font-size: 0.68rem;
    padding: 0.12rem 0.5rem;
    border-radius: 99px;
    background: var(--bg-hover);
    color: var(--fg-dim);
  }
  .badge.warn { background: var(--warn-bg); color: var(--warn); }
  .badge.acc { background: var(--accent-bg); color: var(--accent); }
  .badge.cat-Critical { background: var(--danger-bg); color: var(--danger); }
  .badge.cat-SystemService { background: var(--warn-bg); color: var(--warn); }
  .badge.cat-UserApp { background: var(--accent-bg); color: var(--accent); }
  .metrics {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .metrics div {
    display: flex;
    flex-direction: column;
    background: var(--bg-group);
    border-radius: 8px;
    padding: 0.45rem 0.6rem;
  }
  .metrics span {
    font-size: 0.68rem;
    color: var(--fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .metrics b {
    font-size: 0.95rem;
    font-variant-numeric: tabular-nums;
  }
  .kv {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .kv .row {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 0.5rem;
    font-size: 0.78rem;
  }
  .kv span {
    color: var(--fg-dim);
  }
  code {
    font-family: 'Cascadia Code', ui-monospace, monospace;
    font-size: 0.74rem;
    color: var(--fg);
    word-break: break-word;
  }
  code.wrap {
    white-space: pre-wrap;
  }
  .cores {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(30px, 1fr));
    gap: 4px;
    margin-bottom: 0.6rem;
  }
  .core {
    aspect-ratio: 1;
    border: 1px solid var(--border);
    background: var(--bg-group);
    color: var(--fg-dim);
    border-radius: 5px;
    font-size: 0.62rem;
    cursor: pointer;
  }
  .core.on {
    background: var(--accent);
    color: #0c1017;
    border-color: var(--accent);
    font-weight: 600;
  }
  .muted {
    font-size: 0.76rem;
    color: var(--fg-dim);
    margin: 0 0 0.6rem;
    line-height: 1.4;
  }
  .rule-form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .rule-form select {
    background: var(--bg-group);
    border: 1px solid var(--border);
    color: var(--fg);
    border-radius: 6px;
    padding: 0.35rem 0.5rem;
    font-size: 0.8rem;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.78rem;
    color: var(--fg-dim);
  }
  .btn.small {
    padding: 0.35rem 0.7rem;
    font-size: 0.78rem;
    align-self: flex-start;
  }
</style>
