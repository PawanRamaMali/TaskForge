<script lang="ts">
  import { store } from './store.svelte';
  import {
    GROUP_ORDER,
    describeValue,
    hourLabel,
    hoursSpan,
    isToggle,
    sameValue,
    type HoursValue,
    type SettingState,
    type SettingValue,
  } from './settings';

  let { onclose }: { onclose: () => void } = $props();

  let loading = $state(true);
  let applying = $state(false);
  // Edits not applied yet, by setting id.
  let draft = $state<Record<string, SettingValue>>({});

  const HOURS = Array.from({ length: 24 }, (_, h) => h);

  $effect(() => {
    store.loadSettings().then(() => (loading = false));
  });

  const groups = $derived(
    GROUP_ORDER.map((g) => ({ name: g, items: store.settings.filter((s) => s.group === g) })).filter(
      (g) => g.items.length
    )
  );

  function current(s: SettingState): SettingValue | null {
    return draft[s.id] ?? s.value;
  }

  function edit(s: SettingState, value: SettingValue) {
    const next = { ...draft };
    if (sameValue(value, s.value)) delete next[s.id];
    else next[s.id] = value;
    draft = next;
  }

  function setHours(s: SettingState, part: 'start' | 'end', hour: number) {
    const base = (current(s) as HoursValue | null) ?? { start: 8, end: 17 };
    edit(s, { ...base, [part]: hour });
  }

  function hoursInvalid(v: SettingValue | null): boolean {
    if (!v || isToggle(v)) return false;
    return v.start === v.end || hoursSpan(v) > 18;
  }

  const changes = $derived(
    Object.entries(draft).map(([id, value]) => ({ id, value }))
  );
  const invalid = $derived(changes.some((c) => hoursInvalid(c.value)));
  const needsAdmin = $derived(
    changes.some((c) => store.settings.find((s) => s.id === c.id)?.needsAdmin)
  );

  function useRecommended() {
    const next = { ...draft };
    for (const s of store.settings) {
      if (s.supported && s.recommended && !sameValue(s.recommended, s.value)) next[s.id] = s.recommended;
    }
    draft = next;
  }

  async function apply() {
    applying = true;
    const results = await store.applySettings(changes);
    // Keep edits that failed so they can be retried.
    const failed = new Set(results.filter((r) => !r.ok).map((r) => r.id));
    const next: Record<string, SettingValue> = {};
    for (const [id, value] of Object.entries(draft)) if (failed.has(id)) next[id] = value;
    draft = next;
    applying = false;
  }

  async function restore(s: SettingState) {
    applying = true;
    await store.applySettings([{ id: s.id, value: null }]);
    const next = { ...draft };
    delete next[s.id];
    draft = next;
    applying = false;
  }
</script>

<div class="overlay" onclick={onclose} role="presentation">
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1" onkeydown={() => {}}>
    <h3>Stability settings</h3>
    <p class="muted">
      Settings that help catch the cause of crashes and avoid surprise restarts. TaskForge saves your
      original values so you can put them back.
    </p>

    <div class="body">
      {#if loading}
        <p class="muted">Reading current settings…</p>
      {:else}
        {#each groups as g (g.name)}
          <h4>{g.name}</h4>
          {#each g.items as s (s.id)}
            {@const value = current(s)}
            {@const changed = s.id in draft}
            <div class="row" class:disabled={!s.supported} class:changed>
              <div class="info">
                <div class="label">
                  {s.label}
                  {#if s.needsAdmin}<span class="badge">admin</span>{/if}
                  {#if s.needsRestart}<span class="badge">restart</span>{/if}
                  {#if changed}<span class="badge accent">not applied</span>{/if}
                </div>
                <div class="desc">{s.description}</div>
                {#if s.note}<div class="note">{s.note}</div>{/if}
                {#if s.supported && s.recommended && !sameValue(s.recommended, value)}
                  <div class="hint">Recommended: {describeValue(s.recommended)}</div>
                {/if}
                {#if s.canRestore}
                  <button class="link" disabled={applying} onclick={() => restore(s)}>Restore original value</button>
                {/if}
              </div>

              <div class="control">
                {#if !s.supported}
                  <span class="muted small">Not available</span>
                {:else if s.kind === 'toggle'}
                  {@const on = isToggle(value) && value.on}
                  <button
                    class="switch"
                    class:on
                    role="switch"
                    aria-checked={on}
                    aria-label={s.label}
                    disabled={applying || value == null}
                    onclick={() => edit(s, { on: !on })}
                  >
                    <span class="knob"></span>
                  </button>
                {:else}
                  {@const h = value && !isToggle(value) ? value : null}
                  <div class="hours">
                    <select
                      value={h?.start ?? ''}
                      disabled={applying}
                      onchange={(e) => setHours(s, 'start', Number(e.currentTarget.value))}
                    >
                      {#if !h}<option value="">-</option>{/if}
                      {#each HOURS as hour}<option value={hour}>{hourLabel(hour)}</option>{/each}
                    </select>
                    <span>to</span>
                    <select
                      value={h?.end ?? ''}
                      disabled={applying}
                      onchange={(e) => setHours(s, 'end', Number(e.currentTarget.value))}
                    >
                      {#if !h}<option value="">-</option>{/if}
                      {#each HOURS as hour}<option value={hour}>{hourLabel(hour)}</option>{/each}
                    </select>
                  </div>
                  {#if h}
                    <div class="small" class:fail={hoursInvalid(h)}>
                      {hoursInvalid(h) ? 'Pick 1 to 18 hours' : `${hoursSpan(h)} hours`}
                    </div>
                  {/if}
                {/if}
              </div>
            </div>
          {/each}
        {/each}
      {/if}
    </div>

    <div class="footer">
      {#if needsAdmin}<span class="muted small">Windows will ask for administrator permission once.</span>{/if}
      <div class="buttons">
        <button class="btn" disabled={loading || applying} onclick={useRecommended}>Use recommended</button>
        <button class="btn" onclick={onclose}>Close</button>
        <button class="btn primary" disabled={!changes.length || invalid || applying} onclick={apply}>
          {applying ? 'Applying…' : `Apply ${changes.length || ''} change${changes.length === 1 ? '' : 's'}`}
        </button>
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
    width: min(760px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
  }
  h3 { margin: 0 0 0.3rem; font-size: 1rem; }
  h4 { margin: 1rem 0 0.3rem; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--fg-dim); }
  .muted { color: var(--fg-dim); font-size: 0.82rem; margin: 0 0 0.4rem; }
  .small { font-size: 0.72rem; }
  .fail { color: var(--danger); }
  .body { overflow-y: auto; flex: 1; min-height: 0; padding-right: 0.3rem; }

  .row {
    display: flex;
    gap: 1rem;
    align-items: flex-start;
    padding: 0.6rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-raised);
    margin-bottom: 0.4rem;
  }
  .row.changed { border-color: var(--accent); }
  .row.disabled { opacity: 0.6; }
  .info { flex: 1; min-width: 0; }
  .label { font-size: 0.86rem; font-weight: 600; display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
  .desc { font-size: 0.78rem; color: var(--fg-dim); margin-top: 0.15rem; }
  .note { font-size: 0.74rem; color: var(--warn); margin-top: 0.2rem; }
  .hint { font-size: 0.74rem; color: var(--accent); margin-top: 0.2rem; }
  .badge {
    font-size: 0.62rem; font-weight: 600; padding: 0.05rem 0.4rem; border-radius: 99px;
    background: var(--bg-hover); color: var(--fg-dim);
  }
  .badge.accent { background: var(--accent-bg); color: var(--accent); }
  .link { background: none; border: none; color: var(--accent); cursor: pointer; font-size: 0.74rem; padding: 0; margin-top: 0.3rem; }
  .link:hover:not(:disabled) { text-decoration: underline; }
  .link:disabled { opacity: 0.5; cursor: default; }

  .control { display: flex; flex-direction: column; align-items: flex-end; gap: 0.3rem; flex: none; }
  .switch {
    width: 38px; height: 22px; border-radius: 99px; border: 1px solid var(--border);
    background: var(--bg-hover); position: relative; cursor: pointer; padding: 0;
  }
  .switch .knob {
    position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%;
    background: var(--fg-dim); transition: left 0.15s ease;
  }
  .switch.on { background: var(--accent); border-color: var(--accent); }
  .switch.on .knob { left: 18px; background: #fff; }
  .switch:disabled { opacity: 0.5; cursor: default; }
  .hours { display: flex; align-items: center; gap: 0.35rem; font-size: 0.78rem; }
  select {
    background: var(--bg-menu); color: var(--fg); border: 1px solid var(--border); border-radius: 6px;
    padding: 0.2rem 0.3rem; font-size: 0.78rem;
  }

  .footer { display: flex; align-items: center; gap: 0.8rem; margin-top: 0.9rem; }
  .buttons { display: flex; gap: 0.6rem; margin-left: auto; }
</style>
