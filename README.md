<p align="center">
  <a href="docs/banner.png"><img src="docs/banner.png" alt="Majra — every link lands here" width="100%" /></a>
</p>

<h1 align="center">Majra · مجرى</h1>

<p align="center">
  <strong>Every link lands here.</strong><br />
  A free, open-source download manager for Windows that puts Wi-Fi, Ethernet, USB tethering and
  mobile data to work on the same file.
</p>

<p align="center">
  <a href="https://github.com/bouadjadjatamer2-hub/Majra/releases/download/v1.2.3/Majra-1.2.3-Setup.exe"><img alt="Download Majra for Windows" src="https://img.shields.io/badge/Download%20for%20Windows%20v1.2.3-B45309?style=for-the-badge&logo=windows11&logoColor=white" /></a>
</p>

<p align="center">
  <a href="https://github.com/bouadjadjatamer2-hub/Majra"><img alt="Majra app page" src="https://img.shields.io/badge/App_page-Majra-1F7D5C?style=flat-square&logo=github&logoColor=white" /></a>
  <a href="https://github.com/bouadjadjatamer2-hub/Majra/releases/tag/v1.2.3"><img alt="GitHub Release" src="https://img.shields.io/badge/Release-v1.2.3-24292F?style=flat-square&logo=git&logoColor=white" /></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-4CC38A?style=flat-square" /></a>
  <a href="https://github.com/bouadjadjatamer2-hub/Majra/releases"><img alt="Total downloads" src="https://img.shields.io/github/downloads/bouadjadjatamer2-hub/Majra/total?style=flat-square&label=downloads&color=1F7D5C" /></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=flat-square&logo=windows11&logoColor=white" />
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#features">Features</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#honest-limits">Limits</a> ·
  <a href="#build-from-source">Build from source</a> ·
  <a href="#faq">FAQ</a> ·
  <a href="README.ar.md">العربية</a>
</p>

<a href="docs/screenshots/app-detail.png"><img src="docs/screenshots/app-detail.png" alt="Majra downloading a ZIP over Wi-Fi, USB tethering and Ethernet at the same time" width="100%" /></a>

## Why Majra

Most download managers open several connections to one server over one network. Majra opens
connections over **every network interface Windows exposes**, binds each stream to an interface, and
reassembles the blocks in order. A phone on USB tethering, a Wi-Fi link and an Ethernet cable can
carry the same archive at the same time.

It is built for real-world links: signed-in pages, byte-range servers, very large archives, and
connections that come and go. No account, no ads, no telemetry, no bundled toolbars.

## Features

| Feature                        | What it does                                                                                            |
| ------------------------------ | ------------------------------------------------------------------------------------------------------- |
| **Every connection, together** | Wi-Fi, Ethernet, USB tethering and mobile data can carry the same file at once.                         |
| **2 to 64 connections**        | Pick 2, 4, 8, 16 or 32 parallel connections per network. Turbo mode scales to 64.                       |
| **Download + extract**         | ZIP, RAR and TAR archives — including `.tar.gz`, `.tar.bz2` and `.tar.xz` — unpack while they arrive.   |
| **Direct links stay direct**   | Paste a file URL and go. No in-between page, no forced account. Signed-in downloads keep their cookies. |
| **Browser hand-off**           | The Chromium extension sends a download to Majra with the browser's cookies, then gets out of the way.  |
| **Torrents and magnets**       | Open a `.torrent` or a magnet link as one more source, not as a second app.                             |
| **Resume and schedule**        | Pause, resume after a reboot, and start downloads at a time you choose.                                 |
| **Closes to the tray**         | Keep downloading after the window closes. Start Majra with Windows and it is already there.             |
| **Defender scan**              | Finished files can be scanned with Windows Defender before you open them.                               |
| **Local by design**            | Settings, history and queue live on your PC. Nothing is sent to a server.                               |

## Install

### Windows 10 / 11 · 64-bit

1. Download **[Majra-1.2.3-Setup.exe](https://github.com/bouadjadjatamer2-hub/Majra/releases/download/v1.2.3/Majra-1.2.3-Setup.exe)** (127.8 MB).
2. Run the installer. It is unsigned, so Windows SmartScreen may warn you — choose **More info → Run anyway**, or build from source.
3. Launch Majra, paste a link, and press **Start**.

The installer is an assisted setup: you can choose the folder and the drive, and it creates a desktop shortcut.

### Browser extension

The Chromium extension (Chrome, Edge, Brave, Opera) captures downloads from the browser and hands
them to Majra with the right cookies.

1. Download **[Majra-Browser-Extension.zip](https://github.com/bouadjadjatamer2-hub/Majra/releases/download/v1.2.3/Majra-Browser-Extension.zip)**.
2. Unzip it somewhere permanent.
3. Open `chrome://extensions`, enable **Developer mode**, choose **Load unpacked**, and select the folder.

### Verify the download

```powershell
Get-FileHash .\Majra-1.2.3-Setup.exe -Algorithm SHA256
```

The SHA-256 for `v1.2.3` is:

```text
E009724DD5601171F45874A69539C1464BC051B112472C11B87CCB40F5871B5E
```

## How it works

```text
        Wi-Fi ────────┐
 USB tethering ──────┼──► block scheduler ──► ordered file on disk
      Ethernet ──────┘
```

1. Majra asks Windows for the active network interfaces and their addresses.
2. The file is split into blocks, and each block is requested over the interface with room to spare.
3. Arrived blocks are written in order; missing blocks are re-requested from another stream.
4. When the last block lands, the archive can be extracted in place.

A server that does not support byte ranges cannot be split, so Majra falls back to a single stream
for that link. That is a server limit, not a bug.

## Honest limits

- **Majra cannot create bandwidth that is not there.** Two connections behind the same router often share one uplink; the combined speed will not double.
- **Turbo raises the connection count, not the server's capacity.** Some servers rate-limit or block aggressive parallel requests.
- **Not every link can be split.** Servers without byte-range support fall back to one stream.
- **The installer is unsigned.** There is no code-signing certificate yet, so SmartScreen may ask you to confirm.
- **It is a download manager, not a VPN or a proxy.** It uses the connections Windows already has.

## Build from source

Requirements: **Node.js 22+**, **npm 10+**, and Windows 10/11 for the Windows build.

```powershell
git clone https://github.com/bouadjadjatamer2-hub/Majra.git
cd Majra
npm install
npm run dev
```

| Command                   | What it does                                      |
| ------------------------- | ------------------------------------------------- |
| `npm run dev`             | Start the app in development mode                 |
| `npm run typecheck`       | Type-check main, renderer, e2e and extension code |
| `npm run test:e2e`        | Run the Playwright end-to-end suite               |
| `npm run build`           | Build the app                                     |
| `npm run build:win`       | Build the Windows installer                       |
| `npm run build:extension` | Build the browser extension                       |

Packaging notes live in [REBUILD.md](REBUILD.md), and what is planned next is in
[docs/ROADMAP.md](docs/ROADMAP.md).

## FAQ

<details>
<summary><b>Does Majra need an account?</b></summary>
No. There is no sign-up, no subscription and no ads. The app works offline except for the downloads you start.
</details>

<details>
<summary><b>Does it collect telemetry?</b></summary>
No. Settings, queue and history are stored locally. The app does not send usage data anywhere.
</details>

<details>
<summary><b>Will two networks double my speed?</b></summary>
Only if they are genuinely separate paths to the internet. Wi-Fi and Ethernet behind the same router usually share one uplink, so the gain is small or zero. A phone on mobile data plus a home connection is a real second path.
</details>

<details>
<summary><b>Why does SmartScreen warn me?</b></summary>
The installer is not code-signed. Windows cannot verify the publisher, so it asks you to confirm. You can check the SHA-256 above, or build from source.
</details>

<details>
<summary><b>Can Majra download from YouTube or other streaming sites?</b></summary>
No. It downloads files and torrents, not media streams. There is no bundled media sniffer.
</details>

<details>
<summary><b>Does it work on macOS or Linux?</b></summary>
The source contains build targets for both, but only the Windows installer is published and tested. Treat other platforms as build-it-yourself.
</details>

<details>
<summary><b>Where are my downloads and settings?</b></summary>
Downloads go to the folder you choose (Downloads by default). Settings, queue and history live in the app's user-data folder on the same PC.
</details>

## Contributing

Bug reports and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) first, and open an
issue before starting a large change. Security reports should follow [SECURITY.md](SECURITY.md).

## License

Majra is released under the [MIT License](LICENSE). Third-party notices are in [NOTICE.md](NOTICE.md).

---

<p align="center">
  <sub>Also from the same workbench: <a href="https://github.com/bouadjadjatamer2-hub/DroidCrate"><b>DroidCrate</b></a> — a careful Android service bench for Windows.</sub>
</p>
