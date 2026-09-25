<script lang="ts">
  import { store } from './store.svelte';
  import type { StartupItem } from './startup';

  let { onclose }: { onclose: () => void } = $props();

  let loading = $state(true);
  let busy = $state(false);
  let items = $state<StartupItem[]>([]);

  async function load() {
    loading = true;
    try {
      items = await store.listStartup();
    } catch (e) {
      store.toast('error', `startup: ${e}`);
    }
    loading = false;
  }

  $effect(() => {
    load();
  });

  const enabledCount = $derived(items.filter((i) => i.enabled).length);
  const nonEssentialOn = $derived(items.filter((i) => i.enabled && !i.microsoft));

  async function apply(changes: { id: string; enabled: boolean }[]) {
    if (!changes.length) return;
    busy = true;
    await store.applyStartup(changes);
    await load();
    busy = false;
  }

  const toggle = (i: StartupItem) => apply([{ id: i.id, enabled: !i.enabled }]);
  const disableNonEssential = () => apply(nonEssentialOn.map((i) => ({ id: i.id, enabled: false })));
  const enableAll = () => apply(items.filter((i) => !i.enabled).map((i) => ({ id: i.id, enabled: true })));
</script>

<div class="overlay" onclick={onclose} role="presentation">
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1" onkeydown={() => {}}>
    <h3>Startup apps</h3>
    <p class="muted">
      Everything that launches when you sign in. Turn off what you don't need for a clean, minimal boot;
      core Windows isn't listed here, so disabling these is safe and reversible.
    </p>

    <div class="body">
      {#if loading}
        <p class="muted">Reading startup entries…</p>
      {:else if !items.length}
        <p class="muted">No startup apps found.</p>
      {:else}
        {#each items as i (i.id)}
          <div class="row" class:off={!i.enabled}>
            <div class="info">
              <div class="label">
                {i.name}
                {#if i.microsoft}<span class="badge">Microsoft</span>{/if}
                {#if i.scope === 'machine'}<span class="badge">admin</span>{/if}
                <span class="src">{i.source}</span>
              </div>
              <div class="cmd" title={i.command}>{i.command}</div>
            </div>
            <button
              class="switch"
              class:on={i.enabled}
              role="switch"
              aria-checked={i.enabled}
              aria-label={i.name}
              disabled={busy}
              onclick={() => toggle(i)}
            >
              <span class="knob"></span>
            </button>
          </div>
        {/each}
      {/if}
    </div>

    <div class="footer">
      <span class="muted small">{enabledCount} of {items.length} on{nonEssentialOn.length ? ` · ${nonEssentialOn.length} non-essential` : ''}</span>
      <div class="buttons">
        <button class="btn" disabled={busy || !items.some((i) => !i.enabled)} onclick={enableAll}>Enable all</button>
        <button class="btn" disabled={busy || !nonEssentialOn.length} onclick={disableNonEssential}>
          {busy ? 'Working…' : `Disable non-essential (${nonEssentialOn.length})`}
        </button>
        <button class="btn primary" onclick={onclose}>Close</button>
      </div>
    </div>
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
    width: min(720px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
  }
  h3 { margin: 0 0 0.3rem; font-size: 1rem; }
  .muted { color: var(--fg-dim); font-size: 0.82rem; margin: 0 0 0.4rem; }
  .small { font-size: 0.72rem; margin: 0; }
  .body { overflow-y: auto; flex: 1; min-height: 0; padding-right: 0.3rem; }
  .row {
    display: flex; gap: 1rem; align-items: center;
    padding: 0.55rem 0.7rem; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-raised); margin-bottom: 0.4rem;
  }
  .row.off { opacity: 0.6; }
  .info { flex: 1; min-width: 0; }
  .label { font-size: 0.86rem; font-weight: 600; display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
  .src { font-size: 0.7rem; color: var(--fg-faint); font-weight: 400; }
  .cmd {
    font-size: 0.73rem; color: var(--fg-dim); margin-top: 0.15rem;
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .badge {
    font-size: 0.62rem; font-weight: 600; padding: 0.05rem 0.4rem; border-radius: 99px;
    background: var(--bg-hover); color: var(--fg-dim);
  }
  .switch {
    width: 38px; height: 22px; border-radius: 99px; border: 1px solid var(--border);
    background: var(--bg-hover); position: relative; cursor: pointer; padding: 0; flex: none;
  }
  .switch .knob {
    position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%;
    background: var(--fg-dim); transition: left 0.15s ease;
  }
  .switch.on { background: var(--accent); border-color: var(--accent); }
  .switch.on .knob { left: 18px; background: #fff; }
  .switch:disabled { opacity: 0.5; cursor: default; }
  .footer { display: flex; align-items: center; gap: 0.8rem; margin-top: 0.9rem; }
  .buttons { display: flex; gap: 0.6rem; margin-left: auto; }
</style>
