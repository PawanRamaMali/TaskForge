# TaskForge

A cross-platform task manager (Windows 11 + Ubuntu) built with Tauri v2, Rust, and Svelte.

## Features

- **Live dashboard** — CPU (overall + per-core), RAM, per-drive disk activity, GPU utilization/VRAM/temperature (NVIDIA via NVML; gracefully hidden on machines without it), refreshed every 1.5 s with 90-second sparkline history.
- **Process list** — sortable and searchable, with per-process CPU %, memory, disk I/O, and GPU usage.
- **Smart grouping** — processes are classified as **Apps**, **Background**, **OS services**, or **Critical system**. Critical processes are locked: the backend refuses to kill or suspend them no matter what the UI asks.
- **Per-process actions** — lower priority, trim RAM, suspend/resume, end task, force kill (with confirmation dialogs).
- **Optimize All** — scans for non-essential helpers (updaters, telemetry) and resource hogs, shows a preview of planned actions, and applies only what you leave checked.

## Actions & required privileges

| Action | Windows | Ubuntu |
|---|---|---|
| Lower priority | own processes unelevated; others need admin | own processes (one-way without root) |
| Trim RAM | `EmptyWorkingSet`; services need admin | needs root or `sudo setcap cap_sys_nice+ep <binary>` (kernel ≥ 5.10) |
| Suspend / resume | own processes unelevated; others need admin | own processes (SIGSTOP/SIGCONT) |
| End / force kill | own processes unelevated; others need admin | own processes (SIGTERM/SIGKILL) |

Run elevated (Administrator / sudo) for full control. The header shows a "limited mode" badge otherwise. Protected processes (PPL on Windows, kernel threads on Linux) stay off-limits even when elevated — by design.

## How it compares

Most open-source system monitors are either monitor-only or kill-only. TaskForge focuses on **acting** on processes safely.

| Capability | TaskForge | [NeoHtop](https://github.com/Abdenasser/neohtop) | [system-monitor](https://github.com/xinggaoya/system-monitor) |
|---|:---:|:---:|:---:|
| Live CPU / RAM / disk / GPU | ✅ | ✅ (no GPU) | ✅ |
| Per-process GPU usage | ✅ | ❌ | ❌ (system-wide only) |
| Kill process | ✅ | ✅ | ❌ |
| Lower priority | ✅ | ❌ | ❌ |
| Trim RAM (working set) | ✅ | ❌ | ❌ |
| Suspend / resume | ✅ | ❌ | ❌ |
| OS-critical classification + safety guards | ✅ | ❌ | ❌ |
| "Optimize All" with preview | ✅ | ❌ | ❌ |
| Stack | Tauri + Rust + Svelte | Tauri + Rust + Svelte | Tauri + Rust + Vue |

TaskForge's distinguishing idea is the **guarded action layer**: processes are classified (Critical / OS service / Background / App), the Rust backend refuses destructive actions on protected processes regardless of what the UI requests, and "Optimize All" proposes a reviewable plan rather than acting blindly.

## Development

Prerequisites: Node 20+, Rust stable.

**Windows**: Visual Studio Build Tools 2022 with the C++ workload, WebView2 (preinstalled on Win11).

**Ubuntu**:

```bash
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Then:

```bash
npm install
npm run tauri dev
```

## Building installers

```bash
npm run tauri build
```

Outputs land in `src-tauri/target/release/bundle/`:

- **Windows**: NSIS `.exe` installer + `.msi` (unsigned — SmartScreen will warn; click "More info → Run anyway")
- **Ubuntu**: `.deb` + `.AppImage` (AppImage needs `libfuse2` on stock Ubuntu)

Each OS builds its own installers — build on Windows for Windows, on Ubuntu for Linux. No cross-compilation.

## Architecture

- `src-tauri/src/sampler.rs` — background thread sampling system + process metrics via `sysinfo`, emitting a `snapshot` event every 1.5 s
- `src-tauri/src/classify.rs` — OS-specific classification rules and the safe-to-kill table
- `src-tauri/src/actions/` — per-OS process actions with backend-enforced safety guards
- `src-tauri/src/gpu.rs` — NVML wrapper with graceful degradation
- `src-tauri/src/optimize.rs` — two-phase Optimize All (plan → user preview → apply)
- `src/lib/` — Svelte 5 frontend (stat cards, sparklines, grouped process table, dialogs)
