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
| ✅ | Each account's project list in Canopy |
| ✅ | Two-way folder sync: edit in any editor, Canopy syncs on save and pulls Overleaf's changes; edits on both sides merge |
| ✅ | Snapshots: a local git history of every sync, with diffs and one-click restore |
| ✅ | Local mode: a LaTeX editor on the synced folder, local compiles, a PDF preview, and SyncTeX both ways |
| ✅ | Language server (texlab): completion, hover help, go to definition, problems as you type |
| ✅ | Project search with regular expressions, and a terminal in the project folder |
| ✅ | Zotero citation picker that adds entries to the project's `.bib` |
| ✅ | Writing dashboard: word counts per section, words per day, a goal and a deadline |
| ✅ | Canopy Lite, a smaller web-mode-only download, and a Lite mode switch in the full build |
| ✅ | One-line script install (`install.ps1`, `install.cmd`) that checks every download |

## Install

Every [release](https://github.com/Londopy/canopy/releases/latest) ships:

| Platform | Download |
|---|---|
| Windows | `Canopy_<version>_x64-setup.exe` (**recommended**, per-user, no admin prompt; also x86 and arm64), MSIs, portable exes, a portable zip, and **Canopy Lite** portable exes |
| macOS | `Canopy_<version>_universal.dmg` (Apple silicon and Intel) |
| Linux | `.AppImage`, `.deb` and `.rpm` for x86_64 and arm64 |

Each file also comes as a `.torrent` whose GitHub download is the web seed, so it works even with
nobody seeding.

On Windows, one line picks the right build for your PC, checks its SHA-256 and installs it:

```powershell
irm https://github.com/Londopy/canopy/releases/latest/download/install.ps1 | iex
```

Add options by running it as a script block, for example the portable build, the MSI, or Lite:

```powershell
& ([scriptblock]::Create((irm https://github.com/Londopy/canopy/releases/latest/download/install.ps1))) -Portable
```

The options are `-Portable`, `-Lite`, `-Msi`, `-AllUsers`, `-Version 0.1.0`, `-Dir`, `-AddToPath`,
`-NoShortcut` and `-DryRun`. `install.cmd` does the same for people who'd rather double-click.

**Canopy Lite** is the web mode alone (Overleaf in a window, with accounts, the tray and
updates) in a smaller download: no folder sync, local editor, compiles or terminal. The full
build also has a Lite mode switch in **Settings** for the same thing at run time.

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
| `Ctrl+1` … `Ctrl+6` | Projects, Search, Sync, Snapshots, Dashboard, Editor |
| `Ctrl+S` | Save (and compile, with **Compile on save** on) |
| `Ctrl+Enter` | Compile |
| `Ctrl+Alt+J` | Show the current line in the PDF |
| Double-click the PDF | Jump to that line in the source |
| `Ctrl+,` | Settings |
| `Ctrl+Shift+L` | Switch between light and dark |

### Syncing a project to a folder

In **Projects**, open an account's project list and choose **Sync to a folder** next to a
project. Canopy makes a new folder named after the project inside the one you pick (it won't
sync into a folder that already has files in it) and downloads the project. From then on:

- Save a file in the folder with any editor and Canopy uploads it a couple of seconds later.
  Changes made on Overleaf come down every few minutes, or straight away with **Sync now**.
- Edits to different lines of the same file on both sides merge. Edits to the same lines leave
  `<<<<<<< your folder` / `>>>>>>> overleaf` markers in your copy: keep what you want, delete
  the markers, save, and the next sync uploads it. A file with markers is never uploaded. For
  images and other binary files, Overleaf's version wins and yours is kept next to it as
  `name.local-conflict.ext`.
- Build output (`.aux`, `.log`, `.synctex.gz`, …) and editor or OS litter stay local.
- A sync that would delete every file on one side, or more than half and more than five,
  stops and asks first.
- Files that can't sync (over 64 MB, or a name Windows can't store, like `Figure 3: a.png`)
  are left alone on both sides and listed in **Sync**. A file Overleaf refuses is listed too
  and tried again next time; it doesn't hold up the rest.
- Every sync is a snapshot. **Snapshots** shows what each one changed, with a diff, and can
  put the project back to any of them (on Overleaf too).

A changed document is updated in place (it stays the same document on Overleaf, so its history
continues). Renaming or moving a file in the folder shows up on Overleaf as a delete and a new
file. If someone types in a document on Overleaf in the seconds between Canopy's download and
its upload, the upload wins; the snapshot before it still has their text.

### Editing and compiling on this computer

Open a synced project in **Editor** (`Ctrl+6`, or **Edit here** in **Sync**). It edits the
files in the project's folder, so saving there is just like saving from any other editor:
sync uploads it a moment later.

**Compile** (`Ctrl+Enter`) builds the PDF with the TeX install on this computer, with no free-plan
timeout and no internet needed (Canopy only stops a compile that runs past 5 minutes, which means
it has hung):

- **latexmk** when it works. MiKTeX installs latexmk without the Perl it needs; then Canopy runs
  `pdflatex`, `xelatex` or `lualatex` itself, with BibTeX or Biber and the reruns
  references need.
- **Tectonic** when there's no TeX install.
- XeLaTeX for documents that load `fontspec`, or name a program with `% !TEX program = xelatex`.
- The main file is `main.tex`, or the file with `\documentclass`; pick another under **Main**.

Errors and warnings show under the editor and in the margin; click one to go to its line.
Build output goes to Canopy's own data folder, never into the project.

Project files come from everyone who can edit the project on Overleaf, so a compile never runs
code from them: shell escape is off, latexmk doesn't read the project's `.latexmkrc`, a
`%&format` first line is ignored, no file name is ever taken for a program option, and Biber
runs in the build folder with its config files off.

Saving merges: if a sync changed a file while you had unsaved edits to it, saving combines the
two. If you both changed the same lines, Canopy asks before saving yours over theirs (theirs
stays in **Snapshots** either way). If
a document needs `--shell-escape` (`minted`, for example), compile it on Overleaf.

MiKTeX asks before it installs a missing package. When a compile takes a while, look for its
window: it may be behind Canopy.

### Search, language server, terminal

**Search** (`Ctrl+2`) looks through every text file of a synced project: plain text or a
regular expression, with match case and whole word. Click a result to open it in the editor.

The first time you open the editor, it offers to **Install texlab**, the LaTeX language
server (about 10 MB, from texlab's GitHub releases; Canopy checks it against a pinned
checksum before running it). If `texlab` is already on your PATH, Canopy uses that and doesn't
offer the download. texlab never runs latexmk for Canopy, so a project's `.latexmkrc` can't run
through it either.
With it you get completion for commands, environments, labels and citations (`Ctrl+Space`, or
as you type), hover help for packages and labels, go to definition (`F12`), and problems
marked as you type.

**Terminal** (`` Ctrl+` ``) opens your shell (PowerShell on Windows) in the project's folder,
under the editor.

### Citations from Zotero

**Cite** (`Ctrl+Shift+C`) searches the Zotero library on this computer and adds the entries you
pick to one of the project's `.bib` files (or a new `references.bib`), then types the
`\cite{...}` for you. Zotero has to be running. With the
[Better BibTeX](https://retorque.re/zotero-better-bibtex/) plugin, Canopy uses its citation
keys; without it, Zotero 7 needs one setting: **Settings > Advanced > Allow other applications
on this computer to communicate with Zotero**. Canopy only ever talks to Zotero on this
computer.

### Writing dashboard

The **Dashboard** (`Ctrl+5`) counts the words in a synced project, following `\input` and
`\include` from the main file: per file and per section, words written each day for the last
30 days, and, with a goal and a deadline set, how many words a day you still need. Counting
works like texcount: commands, comments and citations don't count, and math counts as
formulas.

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

[[links]]                 # a synced project; add and remove these from the app
id = "db8a0da933c0"
account_id = "db90a782ebbc"
project_id = "6ae2e6a397229a2a6d2f501c"
project_name = "Thesis"
folder = 'C:\Users\you\Documents\Thesis'
auto_sync = true          # sync on save and every few minutes
```

Snapshots live next to the config, in `sync/<link id>/history.git`: a plain git repository,
so `git log` works on it too.

## How is this different from Overleaf in a browser?

| | Browser | Canopy |
|---|:---:|:---:|
| The real Overleaf editor and collaboration | ✅ | ✅ |
| Self-hosted Overleaf and overleaf.com side by side | ❌ | ✅ |
| Projects as folders on disk | ❌ | ✅ |
| Compiles without the free-plan timeout | ❌ | ✅ |
| Work offline | ❌ | ✅ |
| Version history without premium | ❌ | ✅ |

Canopy is an independent project and isn't affiliated with Overleaf. Overleaf has no public
API, so sync uses the same endpoints the Overleaf web app uses, with the session from the
account's own sign-in (Canopy reads it from that account's browser profile and never stores
it), and that code sits in one crate so it's quick to fix when Overleaf changes something.

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

Sync is tested against a real Overleaf Community Edition running in Docker:

```bash
python dev/overleaf-ce/seed.py --env
docker compose -f dev/overleaf-ce/compose.yml up -d
python dev/overleaf-ce/seed.py
cargo test -p canopy-overleaf -p canopy-sync -- --ignored
```

## Layout

```
src/                     the Svelte + TypeScript UI
  lib/views/             Projects, web mode, Sync, Snapshots, Settings
  lib/components/        activity bar, command palette, status bar, dialogs, toasts
src-tauri/               the Tauri app: commands, accounts, web mode, sync wiring, tray, updater
crates/
  overleaf-client/       everything that talks to an Overleaf server (HTTP + realtime tree)
  sync/                  three-way sync planner, executor and git snapshot store
  texkit/                finding and running the local TeX toolchain
dev/overleaf-ce/         a throwaway Overleaf CE in Docker for end-to-end tests
scripts/                 release_notes.py, check_versions.py
```

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md), and add a line
under `Unreleased` in [CHANGELOG.md](CHANGELOG.md) for anything a user would notice.

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
