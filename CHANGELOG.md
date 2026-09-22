# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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

### Changed
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
