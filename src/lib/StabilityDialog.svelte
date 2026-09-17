<script lang="ts">
  import { store } from './store.svelte';
  import { fmtTs, toMarkdown, type Severity, type StabilityReport } from './diagnostics';
  import { sameValue, type SettingFix } from './settings';

  let { onclose }: { onclose: () => void } = $props();

  const WINDOWS = [7, 30, 60, 180];

  let windowDays = $state(60);
  let phase = $state<'running' | 'report' | 'error'>('running');
  let report = $state<StabilityReport | null>(null);
  let error = $state('');
  let expanded = $state<Set<string>>(new Set());
  let showAllTimeline = $state(false);

  const SEVERITY_LABEL: Record<Severity, string> = {
    critical: 'Critical',
    warning: 'Warning',
    info: 'Info',
    ok: 'OK',
  };

  let applyingFixes = $state(false);

  /** Suggested settings that aren't in place yet (only ones this system supports). */
  function openFixes(fixes: SettingFix[]): SettingFix[] {
    return fixes.filter((x) => {
      const s = store.settings.find((s) => s.id === x.id);
      return s != null && s.supported && !sameValue(s.value, x.value);
    });
  }

  function knownFixes(fixes: SettingFix[]): SettingFix[] {
    return fixes.filter((x) => store.settings.some((s) => s.id === x.id && s.supported));
  }

  async function applyFixes(fixes: SettingFix[]) {
    applyingFixes = true;
    await store.applySettings(fixes.map((x) => ({ id: x.id, value: x.value })));
    applyingFixes = false;
  }

  async function run() {
    phase = 'running';
    error = '';
    store.loadSettings();
    try {
      const r = await store.runDiagnostics(windowDays);
      report = r;
      // Open the findings that matter; leave informational ones collapsed.
      expanded = new Set(r.findings.filter((f) => f.severity === 'critical' || f.severity === 'warning').map((f) => f.id));
      phase = 'report';
    } catch (e) {
      error = String(e);
      phase = 'error';
    }
  }

  $effect(() => {
    run();
  });

  function toggle(id: string) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expanded = next;
  }

  async function copy() {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(toMarkdown(report));
      store.toast('ok', 'report copied to clipboard');
    } catch (e) {
      store.toast('error', `copy failed: ${e}`);
    }
  }

  const timeline = $derived(report ? (showAllTimeline ? report.timeline : report.timeline.slice(0, 12)) : []);
</script>

<div class="overlay" onclick={onclose} role="presentation">
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1" onkeydown={() => {}}>
    <div class="head">
      <h3>Stability diagnostics</h3>
      <label class="window">
        Look back
        <!-- run() reads windowDays inside the $effect below, so changing it re-runs the check. -->
        <select bind:value={windowDays} disabled={phase === 'running'}>
          {#each WINDOWS as d}<option value={d}>{d} days</option>{/each}
        </select>
      </label>
    </div>

    {#if phase === 'running'}
      <div class="body center">
        <div class="spinner"></div>
        <p class="muted">Reading crash records, event logs, drivers and disk health. This takes a few seconds and changes nothing.</p>
      </div>
    {:else if phase === 'error'}
      <div class="body">
        <p class="fail">Diagnostics failed: {error}</p>
      </div>
      <div class="buttons">
        <button class="btn" onclick={onclose}>Close</button>
        <button class="btn primary" onclick={run}>Try again</button>
      </div>
    {:else if report}
      <div class="body">
        <div class="verdict {report.verdict}">
          <span class="pill {report.verdict}">{SEVERITY_LABEL[report.verdict]}</span>
          <span>{report.headline}</span>
        </div>

        <div class="stats">
          {#each report.stats as s}
            <div class="stat {s.value ? s.severity : 'ok'}">
              <span class="stat-value">{s.value}</span>
              <span class="stat-label">{s.label}</span>
            </div>
          {/each}
        </div>

        {#if !report.elevated && report.platform === 'windows'}
          <div class="note">
            Some sources (disk reliability counters, live kernel dumps) need administrator rights.
            <button class="link" onclick={() => store.restartAsAdmin()}>Restart elevated</button>
          </div>
        {/if}

        <h4>Findings</h4>
        {#each report.findings as f (f.id)}
          <div class="finding {f.severity}">
            <button class="finding-head" onclick={() => toggle(f.id)} aria-expanded={expanded.has(f.id)}>
              <span class="pill {f.severity}">{SEVERITY_LABEL[f.severity]}</span>
              <span class="area">{f.area}</span>
              <span class="title">{f.title}</span>
              <span class="chev">{expanded.has(f.id) ? '▾' : '▸'}</span>
            </button>
            {#if expanded.has(f.id)}
              <div class="finding-body">
                <p>{f.detail}</p>
                {#if f.evidence.length}
                  <div class="sub">Evidence</div>
                  <ul class="evidence">
                    {#each f.evidence as e}<li>{e}</li>{/each}
                  </ul>
                {/if}
                {#if f.actions.length}
                  <div class="sub">What to do</div>
                  <ol class="actions">
                    {#each f.actions as a}<li>{a}</li>{/each}
                  </ol>
                {/if}
                {#if f.fixes && knownFixes(f.fixes).length}
                  {@const known = knownFixes(f.fixes)}
                  {@const open = openFixes(known)}
                  <div class="fixes">
                    <span class="fix-list">Settings that help: {known.map((x) => x.label).join(', ')}</span>
                    {#if open.length}
                      <button class="btn small" disabled={applyingFixes} onclick={() => applyFixes(open)}>
                        {applyingFixes ? 'Applying…' : `Apply ${open.length} setting${open.length === 1 ? '' : 's'}`}
                      </button>
                    {:else}
                      <span class="applied">✓ Applied</span>
                    {/if}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}

        {#if report.timeline.length}
          <h4>Timeline</h4>
          <ul class="timeline">
            {#each timeline as t}
              <li>
                <span class="dot {t.severity}"></span>
                <span class="when">{fmtTs(t.ts)}</span>
                <span>{t.label}</span>
              </li>
            {/each}
          </ul>
          {#if report.timeline.length > 12}
            <button class="link" onclick={() => (showAllTimeline = !showAllTimeline)}>
              {showAllTimeline ? 'Show less' : `Show all ${report.timeline.length}`}
            </button>
          {/if}
        {/if}

        <h4>System</h4>
        <dl class="system">
          {#each report.system as [k, v]}
            <dt>{k}</dt>
            <dd>{v}</dd>
          {/each}
        </dl>

        {#each report.notes as n}<p class="muted small">{n}</p>{/each}
      </div>

      <div class="buttons">
        <button class="btn" onclick={copy}>Copy report</button>
        <button class="btn" onclick={() => report && store.saveDiagnosticsReport(report)}>Save report</button>
        <button class="btn" onclick={run}>Re-run</button>
        <button class="btn primary" onclick={onclose}>Close</button>
      </div>
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
    width: min(920px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
  }
  .head { display: flex; align-items: center; gap: 1rem; margin-bottom: 0.6rem; }
  h3 { margin: 0; font-size: 1rem; }
  h4 { margin: 1rem 0 0.4rem; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--fg-dim); }
  .window { margin-left: auto; font-size: 0.78rem; color: var(--fg-dim); display: flex; align-items: center; gap: 0.4rem; }
  select {
    background: var(--bg-raised); color: var(--fg); border: 1px solid var(--border); border-radius: 6px;
    padding: 0.2rem 0.4rem; font-size: 0.78rem;
  }
  .body { overflow-y: auto; flex: 1; min-height: 0; padding-right: 0.3rem; }
  .body.center { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 2.5rem 0; }
  .muted { color: var(--fg-dim); font-size: 0.85rem; text-align: center; max-width: 34rem; }
  .muted.small { font-size: 0.74rem; text-align: left; max-width: none; }
  .fail { color: var(--danger); font-size: 0.85rem; white-space: pre-wrap; }
  .spinner {
    width: 26px; height: 26px; border-radius: 50%; border: 3px solid var(--border);
    border-top-color: var(--accent); animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .verdict {
    display: flex; align-items: center; gap: 0.6rem; padding: 0.7rem 0.8rem; border-radius: 10px;
    font-size: 0.9rem; font-weight: 600; border: 1px solid var(--border);
  }
  .verdict.critical { background: var(--danger-bg); border-color: var(--danger); }
  .verdict.warning { background: var(--warn-bg); border-color: var(--warn); }
  .verdict.info, .verdict.ok { background: var(--ok-bg); border-color: var(--ok); }

  .pill { font-size: 0.66rem; padding: 0.1rem 0.5rem; border-radius: 99px; white-space: nowrap; font-weight: 600; }
  .pill.critical { background: var(--danger); color: #fff; }
  .pill.warning { background: var(--warn); color: #1a1206; }
  .pill.info { background: var(--accent-bg); color: var(--accent); }
  .pill.ok { background: var(--ok-bg); color: var(--ok); }

  .stats { display: grid; grid-template-columns: repeat(6, 1fr); gap: 0.4rem; margin-top: 0.6rem; }
  .stat {
    display: flex; flex-direction: column; padding: 0.45rem 0.55rem; border-radius: 8px;
    background: var(--bg-raised); border: 1px solid var(--border);
  }
  .stat-value { font-size: 1.15rem; font-weight: 700; }
  .stat-label { font-size: 0.66rem; color: var(--fg-dim); }
  .stat.critical .stat-value { color: var(--danger); }
  .stat.warning .stat-value { color: var(--warn); }
  .stat.info .stat-value { color: var(--accent); }
  .stat.ok .stat-value { color: var(--fg-dim); }

  .note {
    margin-top: 0.6rem; font-size: 0.76rem; color: var(--warn); background: var(--warn-bg);
    padding: 0.4rem 0.6rem; border-radius: 8px;
  }
  .link { background: none; border: none; color: var(--accent); cursor: pointer; font-size: 0.76rem; padding: 0; }
  .link:hover { text-decoration: underline; }

  .finding { border: 1px solid var(--border); border-left-width: 3px; border-radius: 8px; margin-bottom: 0.4rem; background: var(--bg-raised); }
  .finding.critical { border-left-color: var(--danger); }
  .finding.warning { border-left-color: var(--warn); }
  .finding.info { border-left-color: var(--accent); }
  .finding.ok { border-left-color: var(--ok); }
  .finding-head {
    display: flex; align-items: center; gap: 0.6rem; width: 100%; padding: 0.55rem 0.7rem;
    background: none; border: none; color: var(--fg); text-align: left; cursor: pointer; font-size: 0.84rem;
  }
  .finding-head:hover { background: var(--bg-hover); border-radius: 8px; }
  .area { font-size: 0.7rem; color: var(--fg-dim); min-width: 4.8rem; }
  .title { flex: 1; font-weight: 600; }
  .chev { color: var(--fg-dim); }
  .finding-body { padding: 0 0.9rem 0.7rem; font-size: 0.8rem; line-height: 1.45; }
  .finding-body p { margin: 0 0 0.5rem; color: var(--fg-dim); }
  .sub { font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--fg-faint); margin: 0.5rem 0 0.2rem; }
  .evidence, .actions { margin: 0; padding-left: 1.2rem; }
  .evidence li { font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace; font-size: 0.72rem; color: var(--fg-dim); margin-bottom: 0.15rem; word-break: break-word; }
  .actions li { margin-bottom: 0.3rem; }
  .fixes {
    display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; margin-top: 0.6rem;
    padding: 0.45rem 0.6rem; border-radius: 8px; background: var(--accent-bg);
  }
  .fix-list { flex: 1; font-size: 0.76rem; }
  .btn.small { padding: 0.25rem 0.6rem; font-size: 0.74rem; }
  .applied { color: var(--ok); font-size: 0.76rem; font-weight: 600; }

  .timeline { list-style: none; margin: 0; padding: 0; font-size: 0.78rem; }
  .timeline li { display: flex; align-items: center; gap: 0.5rem; padding: 0.18rem 0; }
  .when { color: var(--fg-dim); min-width: 11rem; font-variant-numeric: tabular-nums; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .dot.critical { background: var(--danger); }
  .dot.warning { background: var(--warn); }
  .dot.info { background: var(--accent); }
  .dot.ok { background: var(--ok); }

  .system { display: grid; grid-template-columns: max-content 1fr; gap: 0.2rem 1rem; margin: 0; font-size: 0.78rem; }
  .system dt { color: var(--fg-dim); }
  .system dd { margin: 0; word-break: break-word; }

  .buttons { display: flex; justify-content: flex-end; gap: 0.6rem; margin-top: 0.9rem; }
</style>
