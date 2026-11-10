# Majra Next — Rust transfer core + Tauri shell + Svelte UI

This directory is the vertical slice of the next Majra desktop app. It is built beside the
existing Electron app so the shipped `1.2.3` release stays untouched.

## What this slice proves

- **Transfer engine in Rust.** Ranged probe, block planning, parallel range requests,
  bounded writer channel, pause/resume with persisted block state, cancel, retry, and a
  single-stream fallback for servers that ignore `Range`.
- **Thin Tauri shell.** The window hosts the same core through eleven commands; progress
  crosses to the webview as one aggregated event stream at 4 Hz.
- **Original UI.** A dense, IDM-like workspace with Majra's amber/jade identity, built from
  scratch in Svelte 5 and CSS. No Plexo styles, assets, or `window.plexo` calls.
- **Interface bonding.** Wi-Fi, Ethernet and USB tethering adapters are discovered and
  classified; choosing several spreads connections over them so the line is used from more
  than one path. "Use all" selects every adapter in one click. The adapter Windows would use
  on its own is marked *default*, and each download reports throughput per path. When two
  chosen adapters sit on the same IPv4 network, both the dialog and the details panel say so:
  they share one router, so using them together adds no bandwidth.
- **Unpack while downloading.** When the link is a ZIP archive, entries are extracted as
  soon as their bytes form a contiguous prefix on disk, and a final central-directory pass
  fills anything the streaming pass could not describe. Extraction has its own progress,
  entry count and destination folder in the details panel.
- **History that survives a restart.** Every download is written to
  `%APPDATA%\app.majra.desktop\downloads.json`. On the next launch the list comes back,
  transfers that were running are shown as paused, and one *Resume all* button restarts
  them from the saved block map. Completed and failed rows stay in the list until removed.
- **IDM-style queue.** Up to N downloads run at once (default 3, maximum 8, adjustable in
  the sidebar); every extra row shows *Queued* and starts by itself when a slot frees.
  Pausing a queued row removes it from the queue, and resuming still respects the limit.
- **Global speed limit.** One ceiling for the whole list, chosen in the sidebar (256 KB/s
  up to 20 MB/s, or unlimited). The token bucket is shared by every connection, takes
  effect on the next chunk without restarting a row, and costs one atomic load when it is
  off.
- **Low-RAM defaults.** A "4 GB mode" switch is on by default: 32 px rows and no per-row
  host sub-line, which cuts the rendered text and layout work per visible row. Light/dark
  themes and EN/AR RTL are included.

## Layout

```
next/
  core/        pure Rust: probe, plan, net, state, engine, integration tests
  src-tauri/   Tauri 2 shell: commands, event bridge, window config, NSIS bundle
  ui/          Svelte 5 + TypeScript + CSS; virtualized table and details panel
```

## Commands

Run from `next/`:

```powershell
npm install
npm run check                 # svelte-check, 0 errors
npm run build                 # production UI bundle
cargo test                    # unit + end-to-end transfer tests
cargo clippy --all-targets -- -D warnings
npm run tauri build           # NSIS installer under target/release/bundle/nsis
npm run tauri dev             # dev window against the Vite server

# Real download or benchmark through the same core the app uses:
cargo run --release -p majra-core --example fetch -- <url> [destination] [connections] [extract]
```

The browser-only preview (`npm run preview -- --host 127.0.0.1`) runs with seeded demo data
and no real transfers. `?theme=light`, `?lang=ar`, `?lowram=0`, and `?add=1` are accepted for
visual checks.

## Transfer model

1. `probe_url` sends a one-byte ranged GET and records size, validators, filename, and
   range support.
2. `plan` splits the file into bounded blocks (one block when ranges are unavailable).
3. Each connection pulls its block with `Range`; bytes go through a bounded channel to a
   single writer task that seeks and writes, so disk order never blocks the network.
4. Pause cancels workers, drains accepted chunks, then persists completed block indexes to
   `<file>.majra.json`. Resume skips those blocks. Completion removes the state file.
5. Multi-interface mode binds each client to a chosen local address; automatic mode uses
   the Windows route.
6. The writer publishes the contiguous prefix it has written. The extractor walks ZIP local
   headers from zero and unpacks an entry only after its last byte is inside that prefix.
   The final pass reads the central directory for entry totals and for entries that use data
   descriptors, ZIP64 sizes, or other shapes the streaming pass leaves alone.

## Honest limits

- Download speed is bounded by the server and the line. Parallel ranges help on hosts that
  throttle per connection; they do not create bandwidth. Parity with IDM or the current
  Majra must be measured on the same files and network, not assumed.
- Interface bonding depends on the Windows routing table. Binding a socket to a local
  address is the strongest portable hint, but the OS can still send traffic over the same
  default route. Real aggregation (multi-path TCP, per-path congestion control) is a later
  step; the slice gives every selected adapter its share of connections and reports each
  one's throughput.
- Extraction in this slice is ZIP only, with stored and deflate entries. Encrypted archives,
  RAR, 7z and archives that use ZIP64 entry sizes are reported as unsupported and left
  intact; the download itself still completes.
- The writer currently uses seek + write. Sparse files and overlapped `pwrite` are the next
  step for very large files on NTFS.
- This slice has no browser integration, site credentials, video capture, or migration of
  the existing history. Those are separate steps.
- GUI-level automation of a real download still needs a manual click; the core path is
  covered by the local range-server tests, including a ZIP that is unpacked while its last
  blocks are still arriving.

## Measured in this slice

Numbers from the development machine (Windows, 125% DPI), not a promise for every device:

- NSIS installer: 2.77 MiB (`Majra_0.1.0_x64-setup.exe`); app binary 6.43 MiB.
- RAM with the window open and no downloads: about 306 MB working set in total, of which
  the Rust app is about 24 MB and WebView2 owns the rest. WebView2 runs with
  `--disable-gpu`, which removed roughly 90 MB here; the GPU process share is
  machine-dependent, so re-measure on the target laptop.
- 32 MiB ZIP over loopback, 8 connections: 268.8 MB/s, and extraction finished in the
  same run with 2/2 entries. Loopback only proves the pipeline, not internet speed.
- Downloaded ZIP and both unpacked files matched their SHA-256 values after a GUI-driven
  download with "Unpack while downloading" checked.
