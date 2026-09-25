<script lang="ts">
  import { store } from './store.svelte';
  import { fmtBytes, fmtBps, fmtPct, fmtDuration } from './format';
  import {
    CATEGORY_LABELS,
    CATEGORY_ORDER,
    PRIORITY_LEVELS,
    PRIORITY_LABELS,
    type Category,
    type PriorityLevel,
    type ProcessDetails,
    type ProcInfo,
  } from './types';

  let { onselect, selectedPid = null }: { onselect: (pid: number) => void; selectedPid?: number | null } =
    $props();

  let search = $state('');
  let searchDebounced = $state('');
  let debounceTimer: ReturnType<typeof setTimeout>;
  let sortKey = $state<'name' | 'pid' | 'user' | 'cpu' | 'memBytes' | 'disk' | 'gpuUtil'>('cpu');
  let sortDir = $state<1 | -1>(-1);
  let viewMode = $state<'grouped' | 'flat' | 'tree'>('grouped');
  let collapsed = $state<Set<Category>>(new Set(['Critical']));
  let menuPid = $state<number | null>(null);
  let menuPos = $state<{ x: number; y: number } | null>(null);
  let frozen = $state<ProcInfo[] | null>(null);
  let confirm = $state<{ proc: ProcInfo; action: string; label: string } | null>(null);
  // Command line / working directory, fetched on demand when the confirm dialog opens.
  let confirmDetails = $state<ProcessDetails | null>(null);

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

  function cmp(a: ProcInfo, b: ProcInfo): number {
    const va = sortValue(a);
    const vb = sortValue(b);
    const c = typeof va === 'string' ? va.localeCompare(vb as string) : (va as number) - (vb as number);
    return c * sortDir || a.pid - b.pid;
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
    filtered.sort(cmp);
    return filtered;
  });

  const rows = $derived(frozen ?? liveRows);
  const searching = $derived(searchDebounced.trim().length > 0);

  const groups = $derived.by(() => {
    if (viewMode !== 'grouped') return null;
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

  // Tree view: DFS from roots, siblings sorted by the active sort key.
  const treeRows = $derived.by(() => {
    if (viewMode !== 'tree' || searching) return null;
    const pids = new Set(rows.map((p) => p.pid));
    const childrenOf = new Map<number, ProcInfo[]>();
    const roots: ProcInfo[] = [];
    for (const p of rows) {
      if (p.parentPid != null && pids.has(p.parentPid) && p.parentPid !== p.pid) {
        (childrenOf.get(p.parentPid) ?? childrenOf.set(p.parentPid, []).get(p.parentPid)!).push(p);
      } else {
        roots.push(p);
      }
    }
    const out: { proc: ProcInfo; depth: number }[] = [];
    const visited = new Set<number>();
    const walk = (list: ProcInfo[], depth: number) => {
      list.sort(cmp);
      for (const p of list) {
        if (visited.has(p.pid)) continue;
        visited.add(p.pid);
        out.push({ proc: p, depth });
        const kids = childrenOf.get(p.pid);
        if (kids) walk(kids, depth + 1);
      }
    };
    walk(roots, 0);
    return out;
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
    next.has(cat) ? next.delete(cat) : next.add(cat);
    collapsed = next;
  }

  // Opened from the ⋮ button or a right-click on the row; positioned at the cursor.
  function openMenu(pid: number, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (menuPid === pid && e.type !== 'contextmenu') {
      closeMenu();
      return;
    }
    // Approximate menu size, clamped so it stays fully on screen.
    const w = 190;
    const h = 320;
    const x = Math.max(8, Math.min(e.clientX, window.innerWidth - w - 8));
    const y = Math.max(8, Math.min(e.clientY, window.innerHeight - h - 8));
    menuPos = { x, y };
    frozen = liveRows; // freeze sorting while the menu is open
    menuPid = pid;
  }

  function closeMenu() {
    menuPid = null;
    menuPos = null;
    frozen = null;
  }

  const gpuOn = $derived(store.snapshot?.gpu.available ?? false);
  const colCount = $derived(gpuOn ? 9 : 7);

  // Virtualized rendering. The three view modes are flattened into one linear
  // list of display rows, then only the on-screen slice is rendered - the raw
  // process list can be 2000+ rows and putting them all in the DOM on every
  // 1.5s tick is what made the monitor a heavy CPU user.
  type DisplayRow =
    | { kind: 'group'; cat: Category; label: string; count: number; aggCpu: number; aggMem: number; isCollapsed: boolean }
    | { kind: 'proc'; proc: ProcInfo; depth: number };

  const displayRows = $derived.by<DisplayRow[]>(() => {
    const out: DisplayRow[] = [];
    if (groups) {
      for (const g of groups) {
        out.push({ kind: 'group', cat: g.cat, label: g.label, count: g.items.length, aggCpu: g.aggCpu, aggMem: g.aggMem, isCollapsed: collapsed.has(g.cat) });
        if (!collapsed.has(g.cat)) for (const p of g.items) out.push({ kind: 'proc', proc: p, depth: 0 });
      }
    } else if (treeRows) {
      for (const r of treeRows) out.push({ kind: 'proc', proc: r.proc, depth: r.depth });
    } else {
      for (const p of rows) out.push({ kind: 'proc', proc: p, depth: 0 });
    }
    return out;
  });

  const ROW_H = 28; // px; must stay in sync with the fixed row height in CSS
  const OVERSCAN = 8; // rows rendered beyond the viewport on each side
  let scroller = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewportH = $state(640);

  const startIndex = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const endIndex = $derived(Math.min(displayRows.length, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN));
  const visibleRows = $derived(displayRows.slice(startIndex, endIndex));
  const padTop = $derived(startIndex * ROW_H);
  const padBottom = $derived(Math.max(0, (displayRows.length - endIndex) * ROW_H));

  function onScroll() {
    if (scroller) scrollTop = scroller.scrollTop;
    if (menuPid !== null) closeMenu();
  }

  function protectedReason(p: ProcInfo): string | null {
    if (p.category === 'Critical') return 'blocked for critical system processes';
    if (p.category === 'SystemService') return 'blocked for OS services';
    return null;
  }

  function doPriority(p: ProcInfo, level: PriorityLevel) {
    closeMenu();
    store.setPriority(p.pid, level, p.name);
  }

  function doEfficiency(p: ProcInfo) {
    closeMenu();
    store.setEfficiencyMode(p.pid, true, p.name);
  }

  function doSimple(p: ProcInfo, action: string, needsConfirm: boolean, label: string) {
    closeMenu();
    if (!needsConfirm) {
      store.processAction(p.pid, action, p.name);
      return;
    }
    confirm = { proc: p, action, label };
    confirmDetails = null;
    // Enrich with command line / working dir if the process is still selected.
    store.getProcessDetails(p.pid).then((d) => {
      if (confirm?.proc.pid === p.pid) confirmDetails = d;
    });
  }

  function confirmYes() {
    if (!confirm) return;
    const c = confirm;
    confirm = null;
    confirmDetails = null;
    if (c.action === 'kill_tree') store.killTree(c.proc.pid, c.proc.name);
    else store.processAction(c.proc.pid, c.action, c.proc.name);
  }

  // How many descendants a kill-tree would also end, from the current snapshot.
  function descendantCount(pid: number): number {
    const procs = store.snapshot?.processes ?? [];
    const kids = new Map<number, number[]>();
    for (const p of procs) {
      if (p.parentPid != null && p.parentPid !== p.pid) {
        (kids.get(p.parentPid) ?? kids.set(p.parentPid, []).get(p.parentPid)!).push(p.pid);
      }
    }
    const seen = new Set([pid]);
    const stack = [pid];
    let count = 0;
    while (stack.length) {
      for (const k of kids.get(stack.pop()!) ?? []) {
        if (!seen.has(k)) {
          seen.add(k);
          count++;
          stack.push(k);
        }
      }
    }
    return count;
  }

  function rowClick(p: ProcInfo) {
    onselect(p.pid);
  }
</script>

<svelte:window onclick={closeMenu} />

<div class="toolbar">
  <input class="search" type="search" placeholder="Search by name, PID, or user…" bind:value={search} />
  <div class="seg">
    <button class:active={viewMode === 'grouped'} onclick={() => (viewMode = 'grouped')}>Grouped</button>
    <button class:active={viewMode === 'flat'} onclick={() => (viewMode = 'flat')}>Flat</button>
    <button class:active={viewMode === 'tree'} onclick={() => (viewMode = 'tree')} title={searching ? 'Clear search to use tree view' : ''}>Tree</button>
  </div>
  <span class="count">{rows.length} processes</span>
</div>

<div class="table-wrap" bind:this={scroller} bind:clientHeight={viewportH} onscroll={onScroll}>
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
      {#if padTop > 0}<tr class="vspacer" aria-hidden="true"><td colspan={colCount} style="height:{padTop}px"></td></tr>{/if}
      {#each visibleRows as dr (dr.kind === 'group' ? 'g-' + dr.cat : dr.proc.pid)}
        {#if dr.kind === 'group'}
          <tr class="group-row" onclick={() => toggleGroup(dr.cat)}>
            <td colspan={colCount}>
              <span class="chev">{dr.isCollapsed ? '▸' : '▾'}</span>
              <span class="group-label {dr.cat}">{dr.label}</span>
              <span class="group-agg">{dr.count} · CPU {fmtPct(dr.aggCpu)} · {fmtBytes(dr.aggMem)}</span>
            </td>
          </tr>
        {:else}
          {@render row(dr.proc, dr.depth)}
        {/if}
      {/each}
      {#if padBottom > 0}<tr class="vspacer" aria-hidden="true"><td colspan={colCount} style="height:{padBottom}px"></td></tr>{/if}
    </tbody>
  </table>
</div>

{#snippet row(p: ProcInfo, depth: number)}
  <tr
    class:suspended={p.suspended}
    class:selected={p.pid === selectedPid}
    onclick={() => rowClick(p)}
    oncontextmenu={(e) => openMenu(p.pid, e)}
    title="Right-click for actions"
  >
    <td class="name" title={p.exe ?? p.name}>
      <span style="padding-left: {depth * 14}px"></span>
      {#if p.category === 'Critical'}<span class="lock" title="Critical system process - protected">🔒</span>{/if}
      {p.name}
      {#if p.suspended}<span class="badge suspended-badge">suspended</span>{/if}
      {#if p.safeToKill}<span class="badge safe-badge" title="Identified as non-essential">optimizable</span>{/if}
    </td>
    <td class="dim">{p.pid}</td>
    <td class="dim user">{p.user ?? '-'}</td>
    <td class="num" class:hot={p.cpu > 20}>{fmtPct(p.cpu)}</td>
    <td class="num">{fmtBytes(p.memBytes)}</td>
    <td class="num dim">{fmtBps(p.diskReadBps + p.diskWriteBps)}</td>
    {#if gpuOn}
      <td class="num">{p.gpuUtil != null ? fmtPct(p.gpuUtil, 0) : '-'}</td>
      <td class="num dim">{p.gpuMemBytes != null ? fmtBytes(p.gpuMemBytes) : '-'}</td>
    {/if}
    <td class="actions-col">
      <button class="kebab" onclick={(e) => openMenu(p.pid, e)} title="Actions">⋮</button>
      {#if menuPid === p.pid}
        {@const why = protectedReason(p)}
        {@const critical = p.category === 'Critical'}
        <div
          class="menu"
          style="left: {menuPos?.x ?? 0}px; top: {menuPos?.y ?? 0}px"
          onclick={(e) => e.stopPropagation()}
          role="menu"
          tabindex="-1"
          onkeydown={() => {}}
        >
          <div class="menu-section">Priority</div>
          <div class="prio-row">
            {#each PRIORITY_LEVELS as lvl}
              <button class="prio" disabled={critical} title={critical ? why : PRIORITY_LABELS[lvl]} onclick={() => doPriority(p, lvl)}>
                {PRIORITY_LABELS[lvl].split(' ')[0][0]}{lvl === 'BelowNormal' || lvl === 'AboveNormal' ? PRIORITY_LABELS[lvl].split(' ')[1][0] : ''}
              </button>
            {/each}
          </div>
          <button class="menu-item" disabled={critical} title={critical ? why : ''} onclick={() => doEfficiency(p)}>Efficiency mode</button>
          <button class="menu-item" disabled={critical} title={critical ? why : ''} onclick={() => doSimple(p, 'trim_ram', false, 'Trim RAM')}>Trim RAM</button>
          {#if p.suspended}
            <button class="menu-item" disabled={!!why} title={why ?? ''} onclick={() => doSimple(p, 'resume', false, 'Resume')}>Resume</button>
          {:else}
            <button class="menu-item" disabled={!!why} title={why ?? ''} onclick={() => doSimple(p, 'suspend', true, 'Suspend')}>Suspend</button>
          {/if}
          <div class="menu-sep"></div>
          <button class="menu-item danger" disabled={!!why} title={why ?? ''} onclick={() => doSimple(p, 'kill', true, 'End task')}>End task</button>
          <button class="menu-item danger" disabled={!!why} title={why ?? ''} onclick={() => doSimple(p, 'kill_tree', true, 'End process tree')}>End process tree</button>
          <button class="menu-item danger" disabled={!!why} title={why ?? ''} onclick={() => doSimple(p, 'force_kill', true, 'Force kill')}>Force kill</button>
        </div>
      {/if}
    </td>
  </tr>
{/snippet}

{#if confirm}
  {@const p = confirm.proc}
  {@const children = confirm.action === 'kill_tree' ? descendantCount(p.pid) : 0}
  <div class="overlay" onclick={() => (confirm = null)} role="presentation">
    <div class="dialog" onclick={(e) => e.stopPropagation()} role="alertdialog" tabindex="-1" onkeydown={() => {}}>
      <h3>{confirm.label}?</h3>
      <p>
        {confirm.label} <strong>{p.name}</strong> (PID {p.pid})?
        {#if confirm.action === 'kill_tree'}This ends the process{#if children} and its {children} child process{children === 1 ? '' : 'es'}{/if}. Unsaved data will be lost.{:else if confirm.action.includes('kill')}Unsaved data in this process will be lost.{/if}
        {#if confirm.action === 'suspend'}The process will freeze until you resume it.{/if}
      </p>
      <dl class="proc-info">
        <dt>Path</dt>
        <dd class="mono">{p.exe ?? confirmDetails?.cmd?.[0] ?? 'unknown'}</dd>
        <dt>User</dt>
        <dd>{p.user ?? '-'} · {CATEGORY_LABELS[p.category]}</dd>
        <dt>Using</dt>
        <dd>
          {fmtPct(p.cpu)} CPU · {fmtBytes(p.memBytes)} RAM{#if p.gpuUtil != null} · {fmtPct(p.gpuUtil, 0)} GPU{/if}{#if p.runTime > 0} · up {fmtDuration(p.runTime)}{/if}
        </dd>
        {#if confirmDetails?.cwd}
          <dt>Folder</dt>
          <dd class="mono">{confirmDetails.cwd}</dd>
        {/if}
        {#if confirmDetails?.cmd?.length}
          <dt>Command</dt>
          <dd class="mono cmd">{confirmDetails.cmd.join(' ')}</dd>
        {/if}
      </dl>
      <div class="dialog-buttons">
        <button class="btn" onclick={() => (confirm = null)}>Cancel</button>
        <button class="btn danger" onclick={confirmYes}>{confirm.label}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .toolbar { display: flex; align-items: center; gap: 0.8rem; padding: 0.6rem 0; }
  .search {
    flex: 1; max-width: 340px; background: var(--bg-raised); border: 1px solid var(--border);
    border-radius: 8px; padding: 0.45rem 0.7rem; color: var(--fg); font-size: 0.85rem;
  }
  .search:focus { outline: none; border-color: var(--accent); }
  .seg { display: flex; border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
  .seg button {
    background: var(--bg-raised); border: none; color: var(--fg-dim); padding: 0.4rem 0.7rem;
    font-size: 0.78rem; cursor: pointer;
  }
  .seg button.active { background: var(--accent); color: #0c1017; font-weight: 600; }
  .count { margin-left: auto; font-size: 0.78rem; color: var(--fg-dim); }
  .table-wrap { overflow-y: auto; flex: 1; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-raised); }
  table { width: 100%; border-collapse: collapse; font-size: 0.82rem; }
  thead { position: sticky; top: 0; background: var(--bg-raised); z-index: 2; }
  th {
    text-align: left; padding: 0.5rem 0.6rem; font-size: 0.72rem; text-transform: uppercase;
    letter-spacing: 0.05em; color: var(--fg-dim); border-bottom: 1px solid var(--border);
    cursor: pointer; user-select: none; white-space: nowrap;
  }
  th.num { text-align: right; }
  td { padding: 0.32rem 0.6rem; border-bottom: 1px solid var(--border-faint); white-space: nowrap; }
  /* Fixed row height keeps the virtual-scroll offsets exact (ROW_H = 28 in script). */
  tbody tr:not(.vspacer) { height: 28px; box-sizing: border-box; }
  .vspacer td { padding: 0; border: none; }
  tbody tr { cursor: pointer; }
  tbody tr:hover:not(.group-row) { background: var(--bg-hover); }
  tr.selected { background: var(--accent-bg) !important; }
  td.name { max-width: 320px; overflow: hidden; text-overflow: ellipsis; }
  td.user { max-width: 130px; overflow: hidden; text-overflow: ellipsis; }
  td.num { text-align: right; font-variant-numeric: tabular-nums; }
  td.num.hot { color: var(--warn); font-weight: 600; }
  .dim { color: var(--fg-dim); }
  tr.suspended td { opacity: 0.55; }
  .group-row td { background: var(--bg-group); cursor: pointer; font-weight: 600; padding: 0.42rem 0.6rem; }
  .chev { display: inline-block; width: 1em; color: var(--fg-dim); }
  .group-label.Critical { color: var(--danger); }
  .group-label.SystemService { color: var(--warn); }
  .group-label.UserApp { color: var(--accent); }
  .group-label.Background { color: var(--fg); }
  .group-agg { font-weight: 400; color: var(--fg-dim); margin-left: 0.8rem; font-size: 0.75rem; }
  .lock { font-size: 0.7rem; margin-right: 0.2rem; }
  .badge { font-size: 0.62rem; padding: 0.05rem 0.35rem; border-radius: 99px; margin-left: 0.4rem; vertical-align: middle; }
  .suspended-badge { background: var(--warn-bg); color: var(--warn); }
  .safe-badge { background: var(--accent-bg); color: var(--accent); }
  .actions-col { width: 2rem; position: relative; }
  .kebab {
    background: none; border: none; color: var(--fg-dim); cursor: pointer; font-size: 1rem;
    padding: 0 0.4rem; border-radius: 6px; visibility: hidden;
  }
  tr:hover .kebab { visibility: visible; }
  .kebab:hover { background: var(--bg-hover); color: var(--fg); }
  .menu {
    position: fixed; background: var(--bg-menu); border: 1px solid var(--border);
    border-radius: 8px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5); z-index: 50; min-width: 170px;
    padding: 0.25rem; display: flex; flex-direction: column;
  }
  .menu-section { font-size: 0.62rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--fg-faint); padding: 0.3rem 0.5rem 0.15rem; }
  .prio-row { display: flex; gap: 3px; padding: 0 0.35rem 0.3rem; }
  .prio {
    flex: 1; background: var(--bg-hover); border: 1px solid var(--border); color: var(--fg);
    border-radius: 5px; padding: 0.25rem 0; font-size: 0.7rem; cursor: pointer;
  }
  .prio:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
  .prio:disabled { color: var(--fg-faint); cursor: not-allowed; }
  .menu-sep { height: 1px; background: var(--border); margin: 0.25rem 0; }
  .menu-item {
    background: none; border: none; color: var(--fg); text-align: left; padding: 0.4rem 0.7rem;
    border-radius: 6px; cursor: pointer; font-size: 0.82rem;
  }
  .menu-item:hover:not(:disabled) { background: var(--bg-hover); }
  .menu-item:disabled { color: var(--fg-faint); cursor: not-allowed; }
  .menu-item.danger:not(:disabled) { color: var(--danger); }
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; z-index: 100; }
  .dialog { background: var(--bg-menu); border: 1px solid var(--border); border-radius: 12px; padding: 1.2rem 1.4rem; max-width: 440px; width: 90%; }
  .dialog h3 { margin: 0 0 0.6rem; font-size: 1rem; }
  .dialog p { margin: 0 0 0.8rem; font-size: 0.85rem; color: var(--fg-dim); line-height: 1.45; }
  .dialog-buttons { display: flex; justify-content: flex-end; gap: 0.6rem; }
  .proc-info {
    display: grid; grid-template-columns: max-content 1fr; gap: 0.25rem 0.7rem;
    margin: 0 0 1rem; padding: 0.6rem 0.7rem; background: var(--bg-raised);
    border: 1px solid var(--border-faint); border-radius: 8px; font-size: 0.76rem;
  }
  .proc-info dt { color: var(--fg-faint); }
  .proc-info dd { margin: 0; color: var(--fg); min-width: 0; overflow-wrap: anywhere; }
  .proc-info .mono { font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace; font-size: 0.72rem; color: var(--fg-dim); }
  .proc-info .cmd { max-height: 3.4rem; overflow-y: auto; }
</style>
