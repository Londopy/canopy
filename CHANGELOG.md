# Changelog

All notable changes to Canopy are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). The section for a version is
what appears on its GitHub release page. Validated in CI by
[patchnotes](https://github.com/Londopy/patchnotes).

## [Unreleased]

### Added

- The app shell: activity bar, command palette (`Ctrl+Shift+P`), status bar, toasts, and light
  and dark themes that follow the system.
- A tray icon, single instance, close to tray, and start at sign-in.
- Settings for theme, Lite mode, the local compiler and updates, saved to `config.toml`, with
  portable mode when a `portable` file sits next to the exe.
- TeX detection: finds `latexmk`, `pdflatex`, Tectonic, `texlab` and `synctex`, and shows which
  compiler a build would use.
- Settings export and import as a `.londopy` file. Accounts stay on the computer they're on.
- Signed in-app updates from GitHub Releases, installed only when you click
  **Install and restart**.
- Overleaf accounts: add overleaf.com or a self-hosted server, rename, remove. Each account has
  its own browser profile, so several accounts on one server stay signed in separately.
- Web mode: the real Overleaf site inside Canopy, with back, forward, reload, all projects and
  open in browser, and a signed-in indicator. New tabs on the account's server stay in Canopy;
  new tabs on other sites open in your browser.
- Project lists: each account's projects in the Projects view.
- Folder sync: sync a project to a new folder. Saving a file uploads it a couple of seconds
  later, Overleaf's changes come down every few minutes, edits to different lines on both sides
  merge, and same-line edits leave conflict markers that are never uploaded. Build output stays
  local, and a sync that would delete most of one side asks first.
- Snapshots: every sync is kept in a local git history, with a per-file diff and one-click
  restore. Restoring saves the folder as it was first, as a "Before restore" snapshot.
- Local mode: a LaTeX editor (CodeMirror) on a synced project's folder, local compiles with
  latexmk, pdfLaTeX/XeLaTeX/LuaLaTeX run directly (for MiKTeX without Perl) or Tectonic, a PDF
  preview, SyncTeX in both directions, and a problems panel from the log. Compiles never run
  code from the project: shell escape is off and `.latexmkrc` isn't read.
- The Canopy logo.
- Project search across a synced project, with regular expressions.
- The texlab language server in the editor: completion, hover, go to definition and live
  problems. Canopy can download texlab for you (a pinned release, checked against its SHA-256).
- A terminal in the project folder under the editor.
- Citations from Zotero: search the local library and add entries to the project's `.bib`, with
  Better BibTeX citation keys when the plugin is installed.
- A writing dashboard: words per file and section, words per day, and a goal with a deadline.
- A list of every keyboard shortcut in Settings.
- `install.ps1` and `install.cmd`: install the right build for your PC from the latest release,
  checked against `SHA256SUMS.txt` (`-Portable`, `-Lite`, `-Msi`, `-AllUsers`, `-Version`).
- Canopy Lite portable builds for Windows (x64, x86, arm64) in every release.
