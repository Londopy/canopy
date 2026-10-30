# Security policy

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x | ✅ |

Only the latest release gets fixes. The in-app updater (**Settings > Updates**) offers it.

## Reporting a vulnerability

Please don't open a public issue. Report it privately through GitHub:
**Security > Report a vulnerability** on https://github.com/Londopy/canopy. If that doesn't
work for you, email **Londopy@protonmail.com** with "Canopy security" in the subject.

Say what you found, how to reproduce it, and what you think the impact is. You'll get an
acknowledgement within 72 hours and an assessment within 7 days. Fixes are credited in the
changelog unless you'd rather not be named.

## Threat model

Canopy holds sign-ins to Overleaf servers and reads and writes project files on disk, so these
count as vulnerabilities:

- An Overleaf session cookie leaving the OS keychain, landing in a log, `config.toml` or a
  `.londopy` export, or being sent to any host other than the server it belongs to.
- A web page shown in Canopy (including an Overleaf page or a self-hosted server's page)
  reaching Canopy's own commands: files, settings, the updater.
- One account's page reading another account's cookies or storage (each account has its own
  browser profile; macOS before 14 is the known exception).
- A project path from a server writing outside the project's local folder (`..`, absolute
  paths, drive letters), or a project download big enough to fill the disk (sync caps single
  files at 64 MB and a project at 1 GB).
- Sync reading or writing outside the linked folder through a symlink or junction (symlinks
  are skipped when reading, and writes refuse to go through one).
- A project file from the server landing in a `.git` folder (or `.GIT`, `GIT~1`, and other
  spellings that reach the same folder), which could plant git hooks.
- Sync uploading a file that still has unresolved conflict markers.
- A project's files running code on this computer during a local compile or through the
  language server (shell escape, a `.latexmkrc`, or anything else that reaches a command
  line), or a compile writing files anywhere but Canopy's build folder.
- A crafted `.londopy` file doing anything but being refused or opened as settings: path
  traversal, unbounded memory, running code.
- The updater installing anything whose signature doesn't verify against the public key
  built into the app, or the texlab download running without matching its pinned SHA-256.
- An Overleaf page reaching the terminal, the language server or Zotero (they answer only
  Canopy's own window).
- The Zotero picker talking to anything but Zotero on this computer (127.0.0.1).
- Plain `http://` used for a server other than this machine.

These are not:

- Anything that needs an attacker who can already run code as you, or write to Canopy's data
  folder or install folder.
- What an Overleaf server does with your projects. Canopy is a client; report server issues
  to Overleaf or to whoever runs your self-hosted instance.
- A document reading another file on this computer into its own PDF during a local compile.
  TeX allows that by default; Canopy turns it off for TeX Live, but MiKTeX ignores the setting.
  The PDF stays in Canopy's build folder and is never uploaded, so this needs you to share the
  PDF yourself.

## Unsigned binaries

Releases aren't code-signed (no certificate). Instead, every release lists the SHA-256 of
each file in `SHA256SUMS.txt`, and in-app updates are signed with a minisign key whose public
half is built into the app. To check a download:

```powershell
(Get-FileHash .\Canopy_0.1.0_x64-setup.exe -Algorithm SHA256).Hash
```

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```
