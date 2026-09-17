<script lang="ts">
  import { store } from '$lib/store.svelte';
  import StatCard from '$lib/StatCard.svelte';
  import ProcessTable from '$lib/ProcessTable.svelte';
  import OptimizeDialog from '$lib/OptimizeDialog.svelte';
  import StabilityDialog from '$lib/StabilityDialog.svelte';
  import SettingsDialog from '$lib/SettingsDialog.svelte';
  import MiniView from '$lib/MiniView.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  // The mini view window loads this same page.
  const isMini = getCurrentWindow().label === 'mini';
  import DetailPanel from '$lib/DetailPanel.svelte';
  import Toasts from '$lib/Toasts.svelte';
  import { fmtBytes, fmtPct, fmtBps } from '$lib/format';

  let showOptimize = $state(false);
  let showStability = $state(false);
  let showSettings = $state(false);
  let showCores = $state(false);
  let selectedPid = $state<number | null>(null);

  $effect(() => {
    store.start();
  });

  const s = $derived(store.snapshot);
  const selectedProc = $derived(
    selectedPid != null ? (s?.processes.find((p) => p.pid === selectedPid) ?? null) : null
  );

  // Auto-close the detail panel if the selected process exits.
  $effect(() => {
    if (selectedPid != null && s && !s.processes.some((p) => p.pid === selectedPid)) {
      selectedPid = null;
    }
  });

  const diskSub = $derived(
    s?.disks.map((d) => `${d.mount} ${fmtBytes(d.total - d.available)}/${fmtBytes(d.total)}`).join(' · ') ?? ''
  );
  const diskMBps = $derived(
    s ? s.disks.reduce((a, d) => a + d.readBps + d.writeBps, 0) / (1024 * 1024) : 0
  );
  const netKBps = $derived(s ? (s.net.rxBps + s.net.txBps) / 1024 : 0);
  const hotTemp = $derived(
    s && s.temps.length ? Math.max(...s.temps.map((t) => t.tempC)) : null
  );
  const cpuSub = $derived(
    s
      ? `${s.cpu.brand} · ${s.cpu.coreCount} threads${hotTemp != null ? ` · ${hotTemp.toFixed(0)}°C` : ''}`
      : ''
  );
</script>

{#if isMini}
<MiniView />
{:else}
<main>
  <header>
    <h1>TaskForge</h1>
    {#if s && !s.elevated}
      <button
        class="elevation"
        title="Some actions on other users' / system processes will be denied. Click to relaunch with full privileges."
        onclick={() => store.restartAsAdmin()}
      >
        limited mode - elevate
      </button>
    {/if}
    <div class="header-actions">
      <button class="icon-btn" title="Toggle light/dark theme" onclick={() => store.toggleTheme()}>
        {store.theme === 'dark' ? '☀' : '☾'}
      </button>
      <button class="btn" onclick={() => (showStability = true)} title="Find out why the system crashes, freezes or hangs">
        Stability check
      </button>
      <button class="btn" onclick={() => (showSettings = true)} title="Crash capture, Fast Startup, update restarts and autostart">
        Settings
      </button>
      <button class="btn" onclick={() => store.setMiniMode(true)} title="Small always-on-top window with the key graphs">
        Mini view
      </button>
      <button class="btn primary" onclick={() => (showOptimize = true)} disabled={!s}>⚡ Optimize all</button>
    </div>
  </header>

  {#if s}
    <section class="cards">
      <StatCard
        title="CPU"
        value={fmtPct(s.cpu.overall, 0)}
        sub={cpuSub}
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
        value={s.gpu.available ? fmtPct(s.gpu.utilization, 0) : '-'}
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
      <StatCard
        title="Network"
        value={netKBps > 1024 ? `${(netKBps / 1024).toFixed(1)} MB/s` : `${netKBps.toFixed(0)} KB/s`}
        sub="↓ {fmtBps(s.net.rxBps)}  ↑ {fmtBps(s.net.txBps)}"
        history={store.netHistory}
        max={100}
        color="#e06b9c"
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

    <div class="workspace">
      <div class="table-col">
        <ProcessTable onselect={(pid) => (selectedPid = pid)} {selectedPid} />
      </div>
      {#if selectedProc}
        <DetailPanel proc={selectedProc} onclose={() => (selectedPid = null)} />
      {/if}
    </div>
  {:else}
    <div class="loading">Gathering system data…</div>
  {/if}
</main>

{#if showOptimize}
  <OptimizeDialog onclose={() => (showOptimize = false)} />
{/if}
{#if showStability}
  <StabilityDialog onclose={() => (showStability = false)} />
{/if}
{#if showSettings}
  <SettingsDialog onclose={() => (showSettings = false)} />
{/if}
{/if}
<Toasts />

<style>
  :global(:root),
  :global(:root[data-theme='dark']) {
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
  :global(:root[data-theme='light']) {
    --bg: #f4f6fa;
    --bg-raised: #ffffff;
    --bg-group: #eef1f6;
    --bg-menu: #ffffff;
    --bg-hover: #e7ebf2;
    --border: #d6dce6;
    --border-faint: #e8ecf2;
    --fg: #1a1f28;
    --fg-dim: #5c6675;
    --fg-faint: #97a0ad;
    --accent: #2f7ce0;
    --accent-bg: #2f7ce01a;
    --danger: #d64545;
    --danger-bg: #d645451a;
    --warn: #c67c1e;
    --warn-bg: #c67c1e1a;
    --ok: #2fa768;
    --ok-bg: #2fa7681a;
  }
  :global(html, body) {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--fg);
    font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
    overflow: hidden;
  }
  main { display: flex; flex-direction: column; height: 100vh; padding: 0.8rem 1rem; box-sizing: border-box; gap: 0.6rem; }
  header { display: flex; align-items: center; gap: 0.8rem; }
  h1 { font-size: 1.05rem; margin: 0; font-weight: 700; letter-spacing: 0.02em; }
  .elevation {
    font-size: 0.68rem; padding: 0.2rem 0.6rem; border-radius: 99px; background: var(--warn-bg);
    color: var(--warn); border: 1px solid transparent; cursor: pointer;
  }
  .elevation:hover { border-color: var(--warn); }
  .header-actions { margin-left: auto; display: flex; align-items: center; gap: 0.5rem; }
  .icon-btn {
    background: var(--bg-raised); border: 1px solid var(--border); color: var(--fg); border-radius: 8px;
    width: 2rem; height: 2rem; cursor: pointer; font-size: 0.9rem;
  }
  .icon-btn:hover { border-color: var(--accent); }
  :global(.btn) {
    background: var(--bg-hover); color: var(--fg); border: 1px solid var(--border); border-radius: 8px;
    padding: 0.45rem 0.9rem; font-size: 0.82rem; cursor: pointer;
  }
  :global(.btn:hover:not(:disabled)) { border-color: var(--accent); }
  :global(.btn.primary) { background: var(--accent); border-color: var(--accent); color: #0c1017; font-weight: 600; }
  :global(.btn.primary:hover:not(:disabled)) { filter: brightness(1.1); }
  :global(.btn.danger) { background: var(--danger); border-color: var(--danger); color: #fff; font-weight: 600; }
  :global(.btn:disabled) { opacity: 0.5; cursor: not-allowed; }
  .cards { display: grid; grid-template-columns: repeat(5, 1fr); gap: 0.6rem; }
  .cores { display: flex; gap: 4px; padding: 0.4rem 0.2rem; align-items: flex-end; height: 56px; }
  .core {
    flex: 1; max-width: 26px; height: 44px; background: var(--bg-raised); border: 1px solid var(--border);
    border-radius: 4px; position: relative; display: flex; flex-direction: column; justify-content: flex-end; overflow: hidden;
  }
  .core-fill { background: var(--accent); transition: height 0.4s ease; }
  .core-label {
    position: absolute; bottom: 1px; width: 100%; text-align: center; font-size: 0.55rem;
    color: var(--fg-dim); mix-blend-mode: difference;
  }
  .workspace { display: flex; gap: 0.6rem; flex: 1; min-height: 0; }
  .table-col { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .loading { display: flex; align-items: center; justify-content: center; flex: 1; color: var(--fg-dim); }
</style>
