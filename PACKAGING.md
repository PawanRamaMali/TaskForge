# Packaging and store submission

TaskForge targets two app stores: **Ubuntu Software (Snap Store)** and the
**Microsoft Store (Windows)**. This document covers how artifacts are built and
what each store submission needs, including the real constraints a process
manager runs into inside store sandboxes.

## Build artifacts (CI)

`.github/workflows/release.yml` builds installers on every `v*` tag:

| Platform | Artifacts |
| --- | --- |
| Linux (ubuntu-22.04) | `.deb`, `.AppImage` |
| Windows | `.msi`, NSIS `.exe` |

`.github/workflows/ci.yml` gates every push/PR on `npm run check` (svelte-check),
`cargo fmt --check`, and `cargo clippy -D warnings`.

Tag a release:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

The `.deb` and `.AppImage` are the practical way to distribute on Linux today
(no sandbox restrictions). The Store paths below add distribution reach but come
with the caveats noted.

## Ubuntu Software (Snap Store)

Recipe: `snap/snapcraft.yaml`. Build and test locally first:

```bash
sudo snap install snapcraft --classic
snapcraft            # builds in an LXD/multipass VM
sudo snap install taskforge_0.1.0_amd64.snap --dangerous
```

Then register the name and upload:

```bash
snapcraft login
snapcraft register taskforge      # one-time; name must be free
snapcraft upload --release=stable taskforge_0.1.0_amd64.snap
```

**Confinement caveat (important).** A process manager needs to see and act on
other processes. Under `strict` confinement the `system-observe` and
`process-control` interfaces cover the monitor and signal-based actions
(suspend/resume, end task), but changing **priority or CPU affinity on other
processes** needs `CAP_SYS_NICE`, which strict confinement does not grant. The
options:

- Ship `strict` and let priority/affinity degrade gracefully to "needs elevation"
  for non-own processes (monitor + kill still work). Simplest to get published.
- Ship `classic` confinement for full capability. This requires a manual store
  review and a justification; classic snaps are approved sparingly.

Decide this before first upload; it changes the store listing and review path.

## Microsoft Store (Windows, MSIX)

Tauri produces `.msi` and NSIS `.exe`, but the Microsoft Store requires **MSIX**.
Wrap the built app with the MSIX Packaging Tool or `makeappx`:

1. Build the app in CI (the `.msi`/binary from `release.yml`).
2. Create an MSIX from the binary (MSIX Packaging Tool, or `makeappx pack`).
3. Sign it with the certificate whose identity matches the Partner Center
   reservation, then submit through Partner Center.

**Elevation caveat.** Store/MSIX apps run in AppContainer and cannot silently run
as administrator, so managing processes owned by other users needs the app's
"Restart as administrator" path (already present) rather than Store-granted
elevation. The monitor and actions on the user's own processes work unelevated.

## What the maintainer must provide

These need accounts/secrets and cannot live in the repo:

- **Snap Store:** an Ubuntu One account, a registered snap name (`taskforge`),
  and the confinement decision above.
- **Microsoft Store:** a Partner Center account, an app identity reservation, and
  a code-signing certificate (add its base64 + password as repo secrets:
  `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`, then extend
  `release.yml` to sign).
- **Bundle identifier:** `src-tauri/tauri.conf.json` currently uses
  `com.alpine.taskforge`. Set this to an identifier you own (for example
  `io.github.pawanramamali.taskforge`) before the first store submission, since
  it becomes the app's permanent identity.
