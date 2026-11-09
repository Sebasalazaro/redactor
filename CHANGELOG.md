# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.1] - 2026-11-09

### Fixed
- A hostname holding several configured terms (say the client's name from
  the global rules and an app name from the engagement) showed every term
  but the client's. All of them are now replaced, also in emails, JWT claims
  and partially masked values.

## [0.2.0] - 2026-11-09

### Added
- `redactor compare`: compares two Burp site map exports of different roles
  and writes a redacted report for access control review, grouped by templated
  endpoint. It reads exports item by item and sends no requests.
- Desktop: a *Compare* page runs the same comparison from the dashboard.
- *Start at login* in Settings (off by default): starts quietly in the tray.
- Settings shows whether the hotkey is active and when it was last received.
- A diagnostic log, `redactor.log` in the config folder: app events and
  errors, never clipboard contents.

### Changed
- Windows: launching redactor opens the dashboard, with its own taskbar icon
  that can be pinned. Closing the window keeps it in the tray for the hotkey
  (explained the first time); *Quit redactor* in the tray or in Settings stops
  it, web view processes included.
- Only one copy runs at a time: launching it again brings the window back
  instead of starting a second copy that could not get the hotkey.

## [0.1.1] - 2026-11-05

### Fixed
- Windows: the app froze, hotkey included, when the *Redact clipboard* button
  or the tray menu had to open a window. Windows are now created off the main
  thread.
- The dashboard now says so when another app holds the hotkey.

## [0.1.0] - 2026-11-05

### Added
- Windows: desktop app in the system tray with an NSIS installer (per user,
  no admin rights), `--clipboard` in the CLI through the native clipboard,
  and `scripts/fetch-model.ps1`. Clipboard writes are excluded from
  clipboard history and Cloud Clipboard sync.
- Releases ship a `SHA256SUMS` file and build provenance attestations.
- Reversible mode: an encrypted vault per profile (AES-256-GCM, key in the
  OS keychain) remembers what was shared; the dashboard's *Restore* page, the
  menu bar and `redactor --restore` put real values back into LLM answers.
  Ambiguous values are listed, never guessed.
- IPv6 detection and masking.
- AI deep scan: `redactor-ner` runs GLiNER models locally with ONNX Runtime;
  the review popup's **Deep scan (AI)** masks names, organizations,
  usernames, addresses and phone numbers, in a worker process that exits
  when idle. `scripts/fetch-model.sh` downloads a pinned, hash-checked model.
- Desktop: single-screen overview with a **Redact clipboard** button; green
  palette.
- Desktop dashboard overview: last redaction (copy again / forget), active
  profile, live memory use with *Free memory*, session stats and section
  summaries. Engagements get a card view, quick creation from the sidebar
  and an effective-rules panel (global vs engagement).
- Privacy settings: keep the last redaction (auto-forget after 15 min by
  default), clear the clipboard when a review is cancelled.
- Engagement display names with spaces and accents; ids are generated.
- Memory tests with a counting allocator and `scripts/measure-memory.sh`.
- Desktop app (Tauri, macOS menu bar): global hotkey, review popup with
  reveal/re-mask and manual masking, engagement switching from the menu bar,
  settings dashboard with a live playground.
- `redactor-core`: `Store` for the shared config directory (atomic, owner-only
  writes) and `Redaction::segments()` for interactive review.
- `redactor-core`: detection engine for HTTP requests/responses, curl, fetch,
  HAR, Burp XML, Postman and plain text.
- JWT decoding with claim-aware partial masking.
- Global configuration with per-engagement profiles.
- `redactor` CLI with stdin, file and clipboard modes, `--report`, `--json`
  and `--init`.
- Golden, leak and determinism tests over fictional fixtures.

### Security
- Updated dependencies with known advisories: `time` (CVE-2026-25727),
  `quick-xml` (via `plist`), `serde_with`, `webpki-root-certs`, and, for the
  desktop build, `vite` and `source-map-js`. CI now runs `cargo-deny`.

### Changed
- The minimum supported Rust version is now 1.88.
- Peak memory of a redaction is ~3x lower (2.8x the input for a HAR export,
  was 10.3x). `Finding::original` was removed; use `Redaction::original`.
- Desktop windows are created on demand and unloaded when closed: idle
  memory drops from 194 MB to 70 MB.

### Fixed
- Dashboard edits made just before closing the window were lost.
- The overview said "nothing yet" after the last redaction expired; it now
  says why it is gone (kept 60 minutes by default).
- Client names now match regardless of case and accents (`Añil Pagos` ↔
  `anilpagos`, `BANCO ÉXITO` ↔ `Banco Éxito`), and names of four or more
  letters match inside hostnames (`acme` in `api.acmemilesapp.com`).
