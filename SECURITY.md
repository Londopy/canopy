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
  paths, drive letters).
- A crafted `.londopy` file doing anything but being refused or opened as settings: path
  traversal, unbounded memory, running code.
- The updater installing anything whose signature doesn't verify against the public key
  built into the app.
- Plain `http://` used for a server other than this machine.

These are not:

- Anything that needs an attacker who can already run code as you, or write to Canopy's data
  folder or install folder.
- What an Overleaf server does with your projects. Canopy is a client; report server issues
  to Overleaf or to whoever runs your self-hosted instance.
- Running `latexmk` or Tectonic on a project you chose to compile. TeX can read files and, with
  shell escape, run programs; compile only projects you trust, as you would on the command
  line.

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
