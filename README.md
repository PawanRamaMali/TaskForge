# TaskForge

A cross-platform task manager (Windows 11 + Ubuntu) built with Tauri v2, Rust, and Svelte.

## Features

**Monitoring**

- **Live dashboard** - CPU (overall + per-core grid), RAM, per-drive disk activity, network throughput (↓/↑), CPU/system temperatures, and GPU utilization/VRAM/temperature (NVIDIA via NVML; gracefully hidden without it). Refreshed every 1.5 s with 90-second sparkline history.
- **Process list** - sortable and searchable, with per-process CPU %, memory, disk I/O, and GPU usage. View as **grouped** (by category), **flat**, or a **parent/child tree**.
- **Process inspector** - click any row for a detail panel: full command line, working directory, parent, start time, uptime, status, current priority, CPU affinity, and environment-variable count.
- **Smart grouping** - processes are classified as **Apps**, **Background**, **OS services**, or **Critical system**. Critical processes are locked: the backend refuses to kill or suspend them no matter what the UI asks.

**Actions** (all with backend safety guards)

- **Priority** - set any level (Idle → High), or toggle Windows **Efficiency mode** (EcoQoS).
- **CPU affinity** - pick exactly which cores a process may run on.
- **Memory** - trim working set / page out unused RAM.
- **Suspend / resume**, **end task**, **force kill**, and **end process tree** (kills children too, still guarding protected processes).
- **Persistent priority rules** (ProBalance-lite) - remember "always run *chrome.exe* at Below-normal" and re-apply automatically whenever that executable starts. Stored in a config file.
- **Optimize All** - scans for non-essential helpers (updaters, telemetry) and sustained resource hogs, shows a preview of planned actions, and applies only what you leave checked.

**Diagnostics**

- **Stability check** - answers "why does this machine crash, freeze or hang?". It reads the OS crash history (unexpected shutdowns, blue screens and their stop codes, hard freezes), graphics driver timeouts/resets, hypervisor and hardware (WHEA/MCE) errors, out-of-memory events, storage and network driver failures, app crashes/hangs, driver and firmware age, and crash-dump settings. It then ranks likely causes with evidence and concrete next steps, links app crashes to GPU errors that happened at the same moment, and shows a timeline. Pick a 7-180 day look-back; copy or save the report as Markdown. Read-only: Windows uses a PowerShell collector over the event logs/CIM, Linux uses the systemd journal. Run elevated for disk reliability counters and live kernel dumps.

- **Stability settings** - one place for the settings that help catch crashes and avoid surprise restarts. Findings in the Stability check can apply them with one click.

  | Group | Windows | Linux |
  |---|---|---|
  | Crash capture | Ctrl+Scroll crash key, automatic memory dump, keep dump files, keep dumps out of disk cleanup | persistent journal, kdump |
  | Startup | Fast Startup on or off | - |
  | Updates | no auto-restart while signed in, restart notifications, active hours | stop unattended-upgrades from rebooting |
  | App | start TaskForge at sign-in | autostart entry |

  Current values are read without admin rights. Changes that need them ask once (UAC on Windows, pkexec on Linux) and run through TaskForge itself in a helper mode that only accepts entries from a built-in list. Your original values are saved in the app config folder and each setting can be restored. The Linux system settings aren't available in the Snap build.

**Quality-of-life**

- System-tray icon with live CPU/RAM tooltip; click to restore the window.
- One-click **Restart as administrator** (Windows `runas` / Linux `pkexec`).
- Light and dark themes (persisted).

## Actions & required privileges

| Action | Windows | Ubuntu |
|---|---|---|
| Set priority / Efficiency mode | own processes unelevated; others need admin | own processes; raising priority needs root |
| CPU affinity | own processes unelevated; others need admin | own processes (`sched_setaffinity`) |
| Trim RAM | `EmptyWorkingSet`; services need admin | needs root or `sudo setcap cap_sys_nice+ep <binary>` (kernel ≥ 5.10) |
| Suspend / resume | own processes unelevated; others need admin | own processes (SIGSTOP/SIGCONT) |
| End / force kill / kill tree | own processes unelevated; others need admin | own processes (SIGTERM/SIGKILL) |

Run elevated (Administrator / sudo) for full control. The header shows a "limited mode - elevate" button otherwise. Protected processes (PPL on Windows, kernel threads on Linux) stay off-limits even when elevated - by design.

## How it compares

The established process power-tools (System Informer, Process Lasso) are Windows-only; the polished cross-platform monitors (NeoHtop, Mission Center) mostly just kill. TaskForge combines the action depth of the former with cross-platform reach and a guarded, preview-first optimize flow.

| Capability | TaskForge | [System Informer](https://github.com/winsiderss/systeminformer) | [Process Lasso](https://bitsum.com/) | [NeoHtop](https://github.com/Abdenasser/neohtop) | [Mission Center](https://gitlab.com/mission-center-devs/mission-center) |
|---|:---:|:---:|:---:|:---:|:---:|
| Platforms | Win + Linux | Windows | Windows | Win/Mac/Linux | Linux |
| Per-process GPU | ✅ | ✅ | ✅ | ❌ | ❌ |
| Priority levels + Efficiency mode | ✅ | ✅ | ✅ | ❌ | ❌ |
| CPU affinity | ✅ | ✅ | ✅ | ❌ | ❌ |
| Trim RAM (working set) | ✅ | ✅ | ✅ | ❌ | ❌ |
| Suspend / resume | ✅ | ✅ | ✅ | ❌ | ❌ |
| Persistent priority rules | ✅ | ❌ | ✅ | ❌ | ❌ |
| OS-critical classification + **enforced** guards | ✅ | ❌ | partial | ❌ | ❌ |
| "Optimize All" with preview | ✅ | ❌ | auto only | ❌ | ❌ |

No single feature here is unique - each ships in some mature tool. TaskForge's distinguishing combination is: **cross-platform** (Tauri), an explicit **Critical / OS-service / Background / App taxonomy** with guards the *backend* enforces (not a dismissible prompt), and an **Optimize All that proposes a reviewable plan** rather than acting automatically. The action layer is modeled on the Process Explorer / System Informer right-click model; the persistent rules and optimize flow are modeled on Process Lasso's ProBalance and Windows Efficiency Mode.

Deliberately out of scope (System Informer's deep-inspection territory): per-process **network** attribution (needs ETW), and the threads/handles/modules inspector.

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

- **Windows**: NSIS `.exe` installer + `.msi` (unsigned - SmartScreen will warn; click "More info → Run anyway")
- **Ubuntu**: `.deb` + `.AppImage` (AppImage needs `libfuse2` on stock Ubuntu)

Each OS builds its own installers - build on Windows for Windows, on Ubuntu for Linux. No cross-compilation.

## Architecture

- `src-tauri/src/sampler.rs` - background thread sampling system + process/network/temperature metrics via `sysinfo`, emitting a `snapshot` event every 1.5 s; also applies persistent priority rules
- `src-tauri/src/classify.rs` - OS-specific classification rules and the safe-to-kill table
- `src-tauri/src/actions/` - per-OS process actions (priority, efficiency, affinity, trim, suspend, kill, elevation) with backend-enforced safety guards
- `src-tauri/src/rules.rs` - persistent per-executable priority rules (loaded from / saved to the app config dir)
- `src-tauri/src/gpu.rs` - NVML wrapper with graceful degradation
- `src-tauri/src/optimize.rs` - two-phase Optimize All (plan → user preview → apply)
- `src/lib/` - Svelte 5 frontend (stat cards, sparklines, process table with grouped/flat/tree views, detail panel, dialogs)
