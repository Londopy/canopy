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
