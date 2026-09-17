<script lang="ts">
  import { availableMonitors, getCurrentWindow } from '@tauri-apps/api/window';
  import { PhysicalPosition } from '@tauri-apps/api/dpi';
  import { store } from './store.svelte';
  import Sparkline from './Sparkline.svelte';
  import { fmtBps, fmtBytes, fmtPct } from './format';

  // Desktop-wide physical pixels: logical values depend on each monitor's scale.
  const POS_KEY = 'taskforge-mini-pos-px';

  const s = $derived(store.snapshot);
  const memPct = $derived(s ? (s.mem.used / s.mem.total) * 100 : 0);
  const diskMBps = $derived(s ? s.disks.reduce((a, d) => a + d.readBps + d.writeBps, 0) / (1024 * 1024) : 0);
  const netBps = $derived(s ? s.net.rxBps + s.net.txBps : 0);
  const hotTemp = $derived(s && s.temps.length ? Math.max(...s.temps.map((t) => t.tempC)) : null);
  // Busiest process right now, skipping the idle pseudo-process.
  const top = $derived.by(() => {
    if (!s) return null;
    let best = null;
    for (const p of s.processes) {
      if (p.pid === 0 || /idle/i.test(p.name)) continue;
      if (!best || p.cpu > best.cpu) best = p;
    }
    return best;
  });

  // Put the window back where it was last left, if that spot is still on a screen.
  $effect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    (async () => {
      try {
        const saved = JSON.parse(localStorage.getItem(POS_KEY) ?? 'null') as { x: number; y: number } | null;
        if (saved && (await onScreen(saved))) await win.setPosition(new PhysicalPosition(saved.x, saved.y));
      } catch {
        // No saved position or storage unavailable: keep the default corner.
      }
      const stop = await win.onMoved(({ payload }) => {
        try {
          localStorage.setItem(POS_KEY, JSON.stringify({ x: payload.x, y: payload.y }));
        } catch {
          // Position just won't be remembered.
        }
      });
      if (disposed) stop();
      else unlisten = stop;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  async function onScreen(p: { x: number; y: number }): Promise<boolean> {
    const monitors = await availableMonitors();
    // Monitor bounds are in the same desktop-wide physical pixels as the saved point.
    return monitors.some((m) => {
      const margin = 60 * m.scaleFactor;
      return (
        p.x >= m.position.x &&
        p.y >= m.position.y &&
        p.x < m.position.x + m.size.width - margin &&
        p.y < m.position.y + m.size.height - margin
      );
    });
  }
</script>

<div class="mini">
  <div class="bar" data-tauri-drag-region>
    <span class="name" data-tauri-drag-region>TaskForge</span>
    <button class="icon" title="Open the full window" onclick={() => store.setMiniMode(false)}>⤢</button>
  </div>

  {#if s}
    <div class="rows">
      <div class="row" title="{s.cpu.brand} · {s.cpu.coreCount} threads">
        <span class="k">CPU</span>
        <span class="v" style="color: #4f9cf9">{fmtPct(s.cpu.overall, 0)}</span>
        <Sparkline data={store.cpuHistory} color="#4f9cf9" height={20} />
      </div>
      <div class="row" title="{fmtBytes(s.mem.used)} / {fmtBytes(s.mem.total)}">
        <span class="k">RAM</span>
        <span class="v" style="color: #b78af5">{fmtPct(memPct, 0)}</span>
        <Sparkline data={store.memHistory} color="#b78af5" height={20} />
      </div>
      {#if s.gpu.available}
        <div class="row" title="{s.gpu.name ?? 'GPU'}{s.gpu.memUsed != null ? ` · ${fmtBytes(s.gpu.memUsed)} / ${fmtBytes(s.gpu.memTotal)}` : ''}">
          <span class="k">GPU</span>
          <span class="v" style="color: #53c98b">
            {fmtPct(s.gpu.utilization, 0)}{#if s.gpu.temperatureC != null}<small class="temp">{s.gpu.temperatureC}°</small>{/if}
          </span>
          <Sparkline data={store.gpuHistory} color="#53c98b" height={20} />
        </div>
      {/if}
      <div class="row" title="Read + write across all drives">
        <span class="k">Disk</span>
        <span class="v" style="color: #f0a35e">{diskMBps.toFixed(1)}<small> MB/s</small></span>
        <Sparkline data={store.diskHistory} max={50} color="#f0a35e" height={20} />
      </div>
      <div class="row" title="↓ {fmtBps(s.net.rxBps)}  ↑ {fmtBps(s.net.txBps)}">
        <span class="k">Net</span>
        <span class="v" style="color: #e06b9c">{netBps ? fmtBps(netBps).replace('/s', '') : '0'}<small>/s</small></span>
        <Sparkline data={store.netHistory} max={100} color="#e06b9c" height={20} />
      </div>
    </div>
    <div class="foot">
      {#if hotTemp != null}<span>{hotTemp.toFixed(0)}°C</span>{/if}
      {#if top}<span class="top" title="Busiest process">{top.name} {fmtPct(top.cpu, 0)}</span>{/if}
    </div>
  {:else}
    <div class="loading">Loading…</div>
  {/if}
</div>

<style>
  .mini {
    height: 100%;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    box-sizing: border-box;
    user-select: none;
  }
  .bar {
    display: flex;
    align-items: center;
    padding: 0.2rem 0.3rem 0.2rem 0.6rem;
    background: var(--bg-raised);
    border-bottom: 1px solid var(--border);
    cursor: move;
  }
  .name {
    flex: 1;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--fg-dim);
  }
  .icon {
    background: none;
    border: none;
    color: var(--fg-dim);
    cursor: pointer;
    font-size: 0.85rem;
    width: 1.5rem;
    height: 1.4rem;
    border-radius: 4px;
  }
  .icon:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }
  .rows {
    display: flex;
    flex-direction: column;
    justify-content: space-evenly;
    gap: 0.2rem;
    padding: 0.35rem 0.55rem 0.2rem;
    flex: 1;
    min-height: 0;
  }
  .row {
    display: grid;
    /* minmax(0, ...) so the canvas can't widen the graph column past the window. */
    grid-template-columns: 2.1rem 4.6rem minmax(0, 1fr);
    align-items: center;
    gap: 0.35rem;
  }
  .row :global(canvas) {
    min-width: 0;
    display: block;
  }
  .k {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-dim);
  }
  .v {
    font-size: 0.86rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
  }
  small {
    font-size: 0.62rem;
    font-weight: 500;
    color: var(--fg-dim);
  }
  .temp {
    margin-left: 0.25rem;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.15rem 0.6rem 0.3rem;
    font-size: 0.66rem;
    color: var(--fg-dim);
  }
  .top {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    color: var(--fg-dim);
  }
</style>
