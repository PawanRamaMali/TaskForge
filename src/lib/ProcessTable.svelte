<script lang="ts">
  import { store } from './store.svelte';
  import { fmtBytes, fmtBps, fmtPct } from './format';
  import { CATEGORY_LABELS, CATEGORY_ORDER, type Category, type ProcInfo } from './types';

  let search = $state('');
  let searchDebounced = $state('');
  let debounceTimer: ReturnType<typeof setTimeout>;
  let sortKey = $state<'name' | 'pid' | 'user' | 'cpu' | 'memBytes' | 'disk' | 'gpuUtil'>('cpu');
  let sortDir = $state<1 | -1>(-1);
  let grouped = $state(true);
  let collapsed = $state<Set<Category>>(new Set(['Critical']));
  let menuPid = $state<number | null>(null);
  let frozen = $state<ProcInfo[] | null>(null);
  let confirm = $state<{ pid: number; name: string; action: string; label: string } | null>(null);

  $effect(() => {
    const v = search;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => (searchDebounced = v), 150);
  });

  function sortValue(p: ProcInfo): number | string {
    switch (sortKey) {
      case 'name': return p.name.toLowerCase();
      case 'pid': return p.pid;
      case 'user': return p.user?.toLowerCase() ?? '';
      case 'cpu': return p.cpu;
      case 'memBytes': return p.memBytes;
      case 'disk': return p.diskReadBps + p.diskWriteBps;
      case 'gpuUtil': return p.gpuUtil ?? -1;
    }
  }

  const liveRows = $derived.by(() => {
    const procs = store.snapshot?.processes ?? [];
    const q = searchDebounced.trim().toLowerCase();
    const filtered = q
      ? procs.filter(
          (p) =>
            p.name.toLowerCase().includes(q) ||
            String(p.pid).includes(q) ||
            (p.user ?? '').toLowerCase().includes(q)
        )
      : procs.slice();
    filtered.sort((a, b) => {
      const va = sortValue(a);
      const vb = sortValue(b);
      const cmp = typeof va === 'string' ? va.localeCompare(vb as string) : (va as number) - (vb as number);
      return cmp * sortDir || a.pid - b.pid;
    });
    return filtered;
  });

  const rows = $derived(frozen ?? liveRows);

  const groups = $derived.by(() => {
    if (!grouped) return null;
    return CATEGORY_ORDER.map((cat) => {
      const items = rows.filter((p) => p.category === cat);
      return {
        cat,
        label: CATEGORY_LABELS[cat],
        items,
        aggCpu: items.reduce((a, p) => a + p.cpu, 0),
        aggMem: items.reduce((a, p) => a + p.memBytes, 0),
      };
    }).filter((g) => g.items.length > 0);
  });

  function setSort(key: typeof sortKey) {
    if (sortKey === key) sortDir = sortDir === 1 ? -1 : 1;
    else {
      sortKey = key;
      sortDir = key === 'name' || key === 'user' ? 1 : -1;
    }
  }

  function toggleGroup(cat: Category) {
    const next = new Set(collapsed);
    if (next.has(cat)) next.delete(cat);
    else next.add(cat);
    collapsed = next;
  }

  function openMenu(pid: number, e: MouseEvent) {
    e.stopPropagation();
    if (menuPid === pid) {
      closeMenu();
      return;
    }
    frozen = liveRows;
    menuPid = pid;
  }

  function closeMenu() {
    menuPid = null;
    frozen = null;
  }

  type MenuItem = { action: string; label: string; danger?: boolean; needsConfirm?: boolean };

  function menuItems(p: ProcInfo): { item: MenuItem; enabled: boolean; why: string }[] {
    const critical = p.category === 'Critical';
    const service = p.category === 'SystemService';
    const killBlocked = critical || service;
    const whyCat = critical
      ? 'blocked for critical system processes'
      : 'blocked for OS services';
    const items: { item: MenuItem; enabled: boolean; why: string }[] = [
      {
        item: { action: 'lower_priority', label: 'Lower priority' },
        enabled: !critical,
        why: critical ? whyCat : '',
      },
      {
        item: { action: 'trim_ram', label: 'Trim RAM' },
        enabled: !critical,
        why: critical ? whyCat : '',
      },
      p.suspended
        ? { item: { action: 'resume', label: 'Resume' }, enabled: !killBlocked, why: killBlocked ? whyCat : '' }
        : {
            item: { action: 'suspend', label: 'Suspend', needsConfirm: true },
            enabled: !killBlocked,
            why: killBlocked ? whyCat : '',
          },
      {
        item: { action: 'kill', label: 'End task', danger: true, needsConfirm: true },
        enabled: !killBlocked,
        why: killBlocked ? whyCat : '',
      },
      {
        item: { action: 'force_kill', label: 'Force kill', danger: true, needsConfirm: true },
        enabled: !killBlocked,
        why: killBlocked ? whyCat : '',
      },
    ];
    return items;
  }

  function runAction(p: ProcInfo, item: MenuItem) {
    closeMenu();
    if (item.needsConfirm) {
      confirm = { pid: p.pid, name: p.name, action: item.action, label: item.label };
    } else {
      store.processAction(p.pid, item.action, p.name);
    }
  }

  function confirmYes() {
    if (confirm) store.processAction(confirm.pid, confirm.action, confirm.name);
    confirm = null;
  }

  const gpuOn = $derived(store.snapshot?.gpu.available ?? false);
  const colCount = $derived(gpuOn ? 9 : 7);
</script>

<svelte:window onclick={closeMenu} />

<div class="toolbar">
  <input
    class="search"
    type="search"
    placeholder="Search by name, PID, or user…"
    bind:value={search}
  />
  <label class="flat-toggle">
    <input type="checkbox" bind:checked={grouped} />
    Group by type
  </label>
  <span class="count">{rows.length} processes</span>
</div>

<div class="table-wrap">
  <table>
    <thead>
      <tr>
        <th class="name" onclick={() => setSort('name')}>Name {sortKey === 'name' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
        <th onclick={() => setSort('pid')}>PID {sortKey === 'pid' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
        <th onclick={() => setSort('user')}>User {sortKey === 'user' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
        <th class="num" onclick={() => setSort('cpu')}>CPU {sortKey === 'cpu' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
        <th class="num" onclick={() => setSort('memBytes')}>Memory {sortKey === 'memBytes' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
        <th class="num" onclick={() => setSort('disk')}>Disk {sortKey === 'disk' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
        {#if gpuOn}
          <th class="num" onclick={() => setSort('gpuUtil')}>GPU {sortKey === 'gpuUtil' ? (sortDir === 1 ? '▲' : '▼') : ''}</th>
          <th class="num">GPU Mem</th>
        {/if}
        <th class="actions-col"></th>
      </tr>
    </thead>
    <tbody>
      {#if groups}
        {#each groups as g (g.cat)}
          <tr class="group-row" onclick={() => toggleGroup(g.cat)}>
            <td colspan={colCount}>
              <span class="chev">{collapsed.has(g.cat) ? '▸' : '▾'}</span>
              <span class="group-label {g.cat}">{g.label}</span>
              <span class="group-agg">
                {g.items.length} · CPU {fmtPct(g.aggCpu)} · {fmtBytes(g.aggMem)}
              </span>
            </td>
          </tr>
          {#if !collapsed.has(g.cat)}
            {#each g.items as p (p.pid)}
              {@render row(p)}
            {/each}
          {/if}
        {/each}
      {:else}
        {#each rows as p (p.pid)}
          {@render row(p)}
        {/each}
      {/if}
    </tbody>
  </table>
</div>

{#snippet row(p: ProcInfo)}
  <tr class:suspended={p.suspended}>
    <td class="name" title={p.exe ?? p.name}>
      {#if p.category === 'Critical'}<span class="lock" title="Critical system process — protected">🔒</span>{/if}
      {p.name}
      {#if p.suspended}<span class="badge suspended-badge">suspended</span>{/if}
      {#if p.safeToKill}<span class="badge safe-badge" title="Identified as non-essential">optimizable</span>{/if}
    </td>
    <td class="dim">{p.pid}</td>
    <td class="dim user">{p.user ?? '—'}</td>
    <td class="num" class:hot={p.cpu > 20}>{fmtPct(p.cpu)}</td>
    <td class="num">{fmtBytes(p.memBytes)}</td>
    <td class="num dim">{fmtBps(p.diskReadBps + p.diskWriteBps)}</td>
    {#if gpuOn}
      <td class="num">{p.gpuUtil != null ? fmtPct(p.gpuUtil, 0) : '—'}</td>
      <td class="num dim">{p.gpuMemBytes != null ? fmtBytes(p.gpuMemBytes) : '—'}</td>
    {/if}
    <td class="actions-col">
      <button class="kebab" onclick={(e) => openMenu(p.pid, e)} title="Actions">⋮</button>
      {#if menuPid === p.pid}
        <div class="menu" onclick={(e) => e.stopPropagation()} role="menu" tabindex="-1" onkeydown={() => {}}>
          {#each menuItems(p) as { item, enabled, why }}
            <button
              class="menu-item"
              class:danger={item.danger}
              disabled={!enabled}
              title={enabled ? '' : why}
              onclick={() => runAction(p, item)}
            >
              {item.label}
            </button>
          {/each}
        </div>
      {/if}
    </td>
  </tr>
{/snippet}

{#if confirm}
  <div class="overlay" onclick={() => (confirm = null)} role="presentation">
    <div class="dialog" onclick={(e) => e.stopPropagation()} role="alertdialog" tabindex="-1" onkeydown={() => {}}>
      <h3>{confirm.label}?</h3>
      <p>
        {confirm.label} <strong>{confirm.name}</strong> (PID {confirm.pid})?
        {#if confirm.action.includes('kill')}Unsaved data in this process will be lost.{/if}
        {#if confirm.action === 'suspend'}The process will freeze until you resume it.{/if}
      </p>
      <div class="dialog-buttons">
        <button class="btn" onclick={() => (confirm = null)}>Cancel</button>
        <button class="btn danger" onclick={confirmYes}>{confirm.label}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    padding: 0.6rem 0;
  }
  .search {
    flex: 1;
    max-width: 340px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.45rem 0.7rem;
    color: var(--fg);
    font-size: 0.85rem;
  }
  .search:focus {
    outline: none;
    border-color: var(--accent);
  }
  .flat-toggle {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.8rem;
    color: var(--fg-dim);
    cursor: pointer;
    user-select: none;
  }
  .count {
    margin-left: auto;
    font-size: 0.78rem;
    color: var(--fg-dim);
  }
  .table-wrap {
    overflow-y: auto;
    flex: 1;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-raised);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.82rem;
  }
  thead {
    position: sticky;
    top: 0;
    background: var(--bg-raised);
    z-index: 2;
  }
  th {
    text-align: left;
    padding: 0.5rem 0.6rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-dim);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  th.num {
    text-align: right;
  }
  td {
    padding: 0.32rem 0.6rem;
    border-bottom: 1px solid var(--border-faint);
    white-space: nowrap;
  }
  td.name {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  td.user {
    max-width: 130px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  td.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  td.num.hot {
    color: var(--warn);
    font-weight: 600;
  }
  .dim {
    color: var(--fg-dim);
  }
  tr.suspended td {
    opacity: 0.55;
  }
  .group-row td {
    background: var(--bg-group);
    cursor: pointer;
    font-weight: 600;
    padding: 0.42rem 0.6rem;
  }
  .chev {
    display: inline-block;
    width: 1em;
    color: var(--fg-dim);
  }
  .group-label.Critical { color: var(--danger); }
  .group-label.SystemService { color: var(--warn); }
  .group-label.UserApp { color: var(--accent); }
  .group-label.Background { color: var(--fg); }
  .group-agg {
    font-weight: 400;
    color: var(--fg-dim);
    margin-left: 0.8rem;
    font-size: 0.75rem;
  }
  .lock {
    font-size: 0.7rem;
    margin-right: 0.2rem;
  }
  .badge {
    font-size: 0.62rem;
    padding: 0.05rem 0.35rem;
    border-radius: 99px;
    margin-left: 0.4rem;
    vertical-align: middle;
  }
  .suspended-badge {
    background: var(--warn-bg);
    color: var(--warn);
  }
  .safe-badge {
    background: var(--accent-bg);
    color: var(--accent);
  }
  .actions-col {
    width: 2rem;
    position: relative;
  }
  .kebab {
    background: none;
    border: none;
    color: var(--fg-dim);
    cursor: pointer;
    font-size: 1rem;
    padding: 0 0.4rem;
    border-radius: 6px;
    visibility: hidden;
  }
  tr:hover .kebab {
    visibility: visible;
  }
  .kebab:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }
  .menu {
    position: absolute;
    right: 1.8rem;
    top: 0;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    z-index: 10;
    min-width: 150px;
    padding: 0.25rem;
    display: flex;
    flex-direction: column;
  }
  .menu-item {
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    padding: 0.4rem 0.7rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.82rem;
  }
  .menu-item:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .menu-item:disabled {
    color: var(--fg-faint);
    cursor: not-allowed;
  }
  .menu-item.danger:not(:disabled) {
    color: var(--danger);
  }
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
    max-width: 420px;
    width: 90%;
  }
  .dialog h3 {
    margin: 0 0 0.6rem;
    font-size: 1rem;
  }
  .dialog p {
    margin: 0 0 1rem;
    font-size: 0.85rem;
    color: var(--fg-dim);
    line-height: 1.45;
  }
  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
  }
</style>
