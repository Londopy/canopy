<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="" width="96">
</p>

<h1 align="center">Canopy</h1>

<p align="center">
  <a href="https://github.com/Londopy/canopy/actions/workflows/ci.yml"><img src="https://github.com/Londopy/canopy/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Londopy/canopy/releases/latest"><img src="https://img.shields.io/github/v/release/Londopy/canopy" alt="Release"></a>
  <a href="https://github.com/Londopy/canopy/releases"><img src="https://img.shields.io/github/downloads/Londopy/canopy/total" alt="Downloads"></a>
  <a href="CHANGELOG.md"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/Londopy/canopy/gh-pages/changelog-badge.json" alt="changelog"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License"></a>
  <img src="https://img.shields.io/badge/built%20with-Tauri%202-24c8db" alt="Tauri 2">
  <img src="https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey" alt="Platforms">
</p>

**Overleaf in a real desktop app, with the things a browser tab can't do.**

Canopy wraps the real Overleaf editor, for overleaf.com and self-hosted Overleaf alike, and adds
what you keep wishing it had: your projects as plain folders on disk, compiles on your own
machine with no timeout, offline editing, version history without a premium plan, and a
Zotero citation picker. If you only want the window and the tray, the Lite mode stays out of
your way.

> **Status: early.** The app shell, settings, TeX detection and the release pipeline are in
> place. Accounts, sync and the local editor land milestone by milestone; the
> [changelog](CHANGELOG.md) says what works today.

## Features

| | |
|---|---|
| ✅ | Native app for Windows, macOS and Linux, with a tray icon, single instance and start at sign-in |
| ✅ | Command palette (`Ctrl+Shift+P`), light and dark themes, keyboard shortcuts for every view |
| ✅ | Finds your TeX setup: `latexmk`/`pdflatex` from TeX Live or MiKTeX, Tectonic, `texlab`, `synctex` |
| ✅ | Settings export and import as a `.londopy` file |
| ✅ | In-app updates, signature-checked, installed only when you say so |
| ✅ | Overleaf accounts: overleaf.com and self-hosted servers, as many as you like, each with its own sign-in |
| ✅ | Web mode: the real Overleaf editor inside Canopy, with back, reload, all projects and open in browser |
| 🚧 | Project list in Canopy itself (comes with sync) |
| 🚧 | Two-way folder sync, with a three-way merge when both sides changed |
| 🚧 | Local snapshots: automatic git history with diff and restore |
| 🚧 | Local mode: editor, PDF preview with SyncTeX, local compiles, language server, project search |
| 🚧 | Zotero citation picker and `.bib` sync |
| 🚧 | Writing dashboard: word counts per section, daily goals, deadlines |
| 🚧 | Canopy Lite, a smaller web-mode-only download |

## Install

Every [release](https://github.com/Londopy/canopy/releases/latest) ships:

| Platform | Download |
|---|---|
| Windows | `Canopy_<version>_x64-setup.exe` (**recommended**, per-user, no admin prompt), MSIs, portable exes and a portable zip |
| macOS | `Canopy_<version>_universal.dmg` (Apple silicon and Intel) |
| Linux | `.AppImage`, `.deb` and `.rpm` for x86_64 and arm64 |

Each file also comes as a `.torrent` whose GitHub download is the web seed, so it works even with
nobody seeding.

The binaries are not code-signed. Windows SmartScreen may say "Windows protected your PC": click
**More info**, then **Run anyway**. On macOS, the first launch is blocked because the app isn't
notarized: open **System Settings > Privacy & Security** and click **Open Anyway** next to Canopy. Every release lists the SHA-256 of each file in `SHA256SUMS.txt`:

```powershell
(Get-FileHash .\Canopy_0.1.0_x64-setup.exe -Algorithm SHA256).Hash
```

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```

Canopy checks GitHub Releases for updates (**Settings > Updates**). Updates are signed, and
the signature is checked before anything installs. Nothing installs until you click
**Install and restart**.

## Usage

Start Canopy, click **Add an Overleaf account**, pick overleaf.com or type your server's
address, and sign in on Overleaf's own page. Canopy never sees your password. Every account
keeps its own browser profile, so two accounts on the same server stay signed in separately.
(On macOS older than 14, accounts share one profile.) Removing an account deletes its profile,
which signs it out; your projects on the server aren't touched.

Press `Ctrl+Shift+P` (`Cmd+Shift+P` on macOS) for every command. Shortcuts work while Canopy's
own UI has focus; once you click into an Overleaf page, its keys are Overleaf's. Links that open
a new tab stay in Canopy when they're on the account's server, and go to your browser otherwise.

| Shortcut | Does |
|---|---|
| `Ctrl+Shift+P` or `Ctrl+K` | Command palette |
| `Ctrl+1` … `Ctrl+5` | Projects, Search, Sync, Snapshots, Dashboard |
| `Ctrl+,` | Settings |
| `Ctrl+Shift+L` | Switch between light and dark |

## .londopy files

Canopy saves settings as `.londopy` files, the format all Londopy apps share. Double-click one
to open it. If it belongs to another Londopy app, it opens there instead. A `.londopy` file is
a zip with a `londopy.toml` inside, so any zip tool can show you what's in it. Settings
exports leave your accounts out: a sign-in stays on the computer it was made on.

## Configuration

| | Path |
|---|---|
| Windows | `%APPDATA%\Canopy\config.toml` |
| macOS | `~/Library/Application Support/Canopy/config.toml` |
| Linux | `$XDG_CONFIG_HOME/canopy/config.toml` (usually `~/.config/canopy/`) |
| Portable | `config.toml` next to the exe, when a file named `portable` sits there |

```toml
[general]
theme = "system"          # "system", "light" or "dark"
lite_mode = false         # web mode only: no local editor, watcher or language server
close_to_tray = true
start_with_system = false

[compile]
engine = "auto"           # "auto", "latexmk", "tectonic" or "remote"

[updater]
auto_update = true
```

## How is this different from Overleaf in a browser?

| | Browser | Canopy |
|---|:---:|:---:|
| The real Overleaf editor and collaboration | ✅ | ✅ |
| Self-hosted Overleaf and overleaf.com side by side | ❌ | ✅ |
| Projects as folders on disk | ❌ | 🚧 |
| Compiles without the free-plan timeout | ❌ | 🚧 |
| Work offline | ❌ | 🚧 |
| Version history without premium | ❌ | 🚧 |

Canopy is an independent project and isn't affiliated with Overleaf. Overleaf has no public
API, so sync uses the same endpoints the Overleaf web app uses, with your own session, and is
kept in one crate so it's quick to fix when Overleaf changes something.

## Building from source

You need [Rust](https://rustup.rs) 1.90+, [Node.js](https://nodejs.org) 22+, and Tauri's
[platform prerequisites](https://v2.tauri.app/start/prerequisites/) (WebView2 on Windows,
webkit2gtk 4.1 on Linux).

```bash
git clone https://github.com/Londopy/canopy
cd canopy
npm ci
npm run tauri:dev      # run with hot reload
npm run tauri:build    # installers for this platform
```

Canopy Lite: `npx tauri build -- --no-default-features`.

## Layout

```
src/                     the Svelte + TypeScript UI
  lib/views/             Projects, Settings, …
  lib/components/        activity bar, command palette, status bar, toasts
src-tauri/               the Tauri app: commands, tray, updater, config, .londopy files
crates/
  overleaf-client/       everything that talks to an Overleaf server
  sync/                  local folder <-> Overleaf project sync
  texkit/                finding and running the local TeX toolchain
scripts/                 release_notes.py, check_versions.py
```

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md), and add a line
under `Unreleased` in [CHANGELOG.md](CHANGELOG.md) for anything a user would notice.

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
