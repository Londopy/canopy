# Contributing to Canopy

Issues and pull requests are welcome.

## Before you open a pull request

- Run the app: `npm ci`, then `npm run tauri:dev`.
- Test: `cargo test --workspace` and `npm run check`.
- Lint: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
  Also lint the Lite build: `cargo clippy -p canopy --no-default-features --all-targets -- -D warnings`.
- Anything that talks to Overleaf goes in `crates/overleaf-client` and nowhere else.
- Sync changes: also run the end-to-end tests against a local Overleaf CE (needs Docker):
  `python dev/overleaf-ce/seed.py --env`, `docker compose -f dev/overleaf-ce/compose.yml up -d`,
  `python dev/overleaf-ce/seed.py`, then `cargo test -p canopy-overleaf -p canopy-sync -- --ignored`.
- Add a line under `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md) if the change is
  something a user would notice. The changelog is checked in CI by
  [patchnotes](https://github.com/Londopy/patchnotes).
- Fill in the pull request template, including the **AI tools** section.
- Sign the [CLA](CLA.md) when the check asks (once).

## AI-assisted contributions

You may use AI tools to help write a contribution. Three rules, which the
CLA makes binding:

- **Say so in the pull request.** The template has a section for it: which
  tool, and what it did (drafted the change, wrote the tests, explained the
  code, translated the docs, wrote the description). "None" is a fine answer.
  A check on every pull request fails until the section is filled in.
- **You are the author.** Read and understand everything the tool produced
  before you submit it; you are answerable for it in review exactly as for
  code you typed. A pull request whose author cannot explain a change is
  sent back. Commits and files name people, not tools: an AI tool is never
  listed as an author or co-author, and commits carry no "Generated with"
  footers. The check rejects both.
- **No laundering.** Do not submit output that reproduces someone else's
  work under a license incompatible with this project's license, or that you
  have no right to license. If a tool hands you a recognizable chunk of
  another project, leave it out.

An undisclosed AI-assisted contribution is closed when discovered, whatever
its quality. Disclosed ones are reviewed like any other, and labeled
`ai-assisted`: the tests are the bar.

The same goes for issues: if an AI tool wrote or summarized a bug report,
say so in it.

## Contributor License Agreement

Your first pull request gets a comment from the CLA check asking you to
accept [`CLA.md`](CLA.md) by replying with one sentence. You keep the
copyright in your work; the agreement gives the project a license to
distribute it under its license (or another OSI-approved license, never a
proprietary one) and a patent license for what your contribution
necessarily uses, and it records that the work is yours to give,
AI-assisted or not. Every author of a commit in the pull request signs once,
and that covers all their later pull requests. Companies contributing on
behalf of employees can open an issue naming the covered accounts instead.
