<script lang="ts">
  import { store } from '$lib/store.svelte';
  import StatCard from '$lib/StatCard.svelte';
  import ProcessTable from '$lib/ProcessTable.svelte';
  import OptimizeDialog from '$lib/OptimizeDialog.svelte';
  import Toasts from '$lib/Toasts.svelte';
  import { fmtBytes, fmtPct } from '$lib/format';

  let showOptimize = $state(false);
  let showCores = $state(false);

  $effect(() => {
    store.start();
  });

  const s = $derived(store.snapshot);
  const diskSub = $derived(
    s?.disks
      .map((d) => `${d.mount} ${fmtBytes(d.total - d.available)}/${fmtBytes(d.total)}`)
      .join(' · ') ?? ''
  );
  const diskMBps = $derived(
    s ? s.disks.reduce((a, d) => a + d.readBps + d.writeBps, 0) / (1024 * 1024) : 0
  );
</script>

<main>
  <header>
    <h1>TaskForge</h1>
    {#if s && !s.elevated}
      <span class="elevation" title="Some actions on other users' / system processes will be denied. Restart as administrator (or with sudo) for full control.">
        limited mode
      </span>
    {/if}
    <button class="btn primary optimize-btn" onclick={() => (showOptimize = true)} disabled={!s}>
      ⚡ Optimize all
    </button>
  </header>

  {#if s}
    <section class="cards">
      <StatCard
        title="CPU"
        value={fmtPct(s.cpu.overall, 0)}
        sub="{s.cpu.brand} · {s.cpu.coreCount} threads"
        history={store.cpuHistory}
        color="#4f9cf9"
        onclick={() => (showCores = !showCores)}
      />
      <StatCard
        title="Memory"
        value={fmtPct((s.mem.used / s.mem.total) * 100, 0)}
        sub="{fmtBytes(s.mem.used)} / {fmtBytes(s.mem.total)}"
        history={store.memHistory}
        color="#b78af5"
      />
      <StatCard
        title="GPU"
        value={s.gpu.available ? fmtPct(s.gpu.utilization, 0) : '—'}
        sub="{s.gpu.name ?? ''}{s.gpu.memUsed != null ? ` · ${fmtBytes(s.gpu.memUsed)} / ${fmtBytes(s.gpu.memTotal)}` : ''}{s.gpu.temperatureC != null ? ` · ${s.gpu.temperatureC}°C` : ''}"
        history={store.gpuHistory}
        color="#53c98b"
        disabled={!s.gpu.available}
        disabledReason={s.gpu.reason ?? 'No GPU telemetry available'}
      />
      <StatCard
        title="Disk"
        value="{diskMBps.toFixed(1)} MB/s"
        sub={diskSub}
        history={store.diskHistory}
        max={50}
        color="#f0a35e"
      />
    </section>

    {#if showCores}
      <section class="cores">
        {#each s.cpu.perCore as pct, i}
          <div class="core" title="Core {i}: {fmtPct(pct)}">
            <div class="core-fill" style="height: {Math.min(100, pct)}%"></div>
            <span class="core-label">{i}</span>
          </div>
        {/each}
      </section>
    {/if}

    <ProcessTable />
  {:else}
    <div class="loading">Gathering system data…</div>
  {/if}
</main>

{#if showOptimize}
  <OptimizeDialog onclose={() => (showOptimize = false)} />
{/if}
<Toasts />

<style>
  :global(:root) {
    --bg: #12141a;
    --bg-raised: #191c24;
    --bg-group: #1e222c;
    --bg-menu: #22262f;
    --bg-hover: #2a2f3a;
    --border: #2b303c;
    --border-faint: #21252e;
    --fg: #dfe3ea;
    --fg-dim: #8b93a3;
    --fg-faint: #565d6b;
    --accent: #4f9cf9;
    --accent-bg: #4f9cf922;
    --danger: #f26d6d;
    --danger-bg: #f26d6d22;
    --warn: #f0b35e;
    --warn-bg: #f0b35e22;
    --ok: #53c98b;
    --ok-bg: #53c98b22;
  }
  :global(html, body) {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--fg);
    font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
    overflow: hidden;
  }
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 0.8rem 1rem;
    box-sizing: border-box;
    gap: 0.6rem;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.8rem;
  }
  h1 {
    font-size: 1.05rem;
    margin: 0;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .elevation {
    font-size: 0.68rem;
    padding: 0.15rem 0.5rem;
    border-radius: 99px;
    background: var(--warn-bg);
    color: var(--warn);
    cursor: help;
  }
  .optimize-btn {
    margin-left: auto;
  }
  :global(.btn) {
    background: var(--bg-hover);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.45rem 0.9rem;
    font-size: 0.82rem;
    cursor: pointer;
  }
  :global(.btn:hover:not(:disabled)) {
    border-color: var(--accent);
  }
  :global(.btn.primary) {
    background: var(--accent);
    border-color: var(--accent);
    color: #0c1017;
    font-weight: 600;
  }
  :global(.btn.primary:hover:not(:disabled)) {
    filter: brightness(1.1);
  }
  :global(.btn.danger) {
    background: var(--danger);
    border-color: var(--danger);
    color: #0c1017;
    font-weight: 600;
  }
  :global(.btn:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.6rem;
  }
  .cores {
    display: flex;
    gap: 4px;
    padding: 0.4rem 0.2rem;
    align-items: flex-end;
    height: 56px;
  }
  .core {
    flex: 1;
    max-width: 26px;
    height: 44px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 4px;
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    overflow: hidden;
  }
  .core-fill {
    background: var(--accent);
    transition: height 0.4s ease;
  }
  .core-label {
    position: absolute;
    bottom: 1px;
    width: 100%;
    text-align: center;
    font-size: 0.55rem;
    color: var(--fg-dim);
    mix-blend-mode: difference;
  }
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--fg-dim);
  }
</style>
