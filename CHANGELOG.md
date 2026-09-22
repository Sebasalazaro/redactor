# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
