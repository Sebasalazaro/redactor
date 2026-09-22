# redactor desktop

Menu bar app for macOS (Windows and Linux should work but are untested). Press
a hotkey anywhere, review the redacted clipboard in a popup and press Enter to
copy it.

![flow](../../docs/desktop-flow.svg)

## Features

- **Global hotkey** (default `⌥⌘R`, configurable) redacts the clipboard.
- **Review popup:** colored diff by data type, click any value to reveal or
  re-mask it, select text and press `M` to mask something the engine missed,
  `Enter` copies and `Esc` cancels.
- **Deep scan (AI):** in the popup, runs the local GLiNER model over the text
  and masks names, organizations, usernames, addresses and phone numbers the
  patterns missed (dashed outline). Needs `scripts/fetch-model.sh` once.
- **Menu bar:** redact now, open the dashboard, switch the active engagement.
- **Dashboard:**
  - *Overview:* the last redaction (copy it again or forget it), the active
    profile, live memory use with a *Free memory* button, session stats and a
    summary of every section.
  - *Engagements:* create one from any name (`Globex Q3 — Web app` is stored
    as `globex-q3-web.toml`), switch from the sidebar, and see the
    **effective rules**: what comes from the global config and what from the
    engagement.
  - *Global rules*, *Playground* and *Settings* (hotkey, review mode,
    privacy and memory options). Every change is saved automatically.
- **Shares its config with the CLI** (`~/.config/redactor`), so both apply the
  same rules.

## Memory

Measured on an Apple Silicon Mac with `scripts/measure-memory.sh` (resident
memory of the app plus the WebKit processes serving its windows):

| State | Before (v0.1) | Now |
|---|---|---|
| Idle in the menu bar | 194 MB | **70 MB** |
| Dashboard open | 194 MB | 170 MB |

The AI model runs in a worker process (the app's own binary started with
`--ner-worker`), created on the first Deep scan and ended after two idle
minutes or by *Free memory*. ONNX Runtime keeps most of its memory after a
model is dropped (~190 MB), so ending the process is the only way to get it
all back; `tests/worker.rs` checks that the process is gone.

Windows are created when needed and destroyed when closed, which ends their
WebKit processes (*Unload windows when closed*, on by default). The engine's
memory is covered by tests with a counting allocator: no bytes retained
across thousands of redactions, and a peak under 5x the input size.

Sensitive text is kept as briefly as possible: the pending redaction only
while the popup is open (and removed from the page before it closes), the
last redaction only as redacted text, in memory, for 15 minutes by default.
*Free memory* drops both and returns freed pages to the OS.

## Security

- The webviews can only call the app's own commands. The clipboard, global
  shortcut and file access plugins run in Rust and are **not** exposed to
  JavaScript (see `src-tauri/capabilities/default.json`).
- Strict CSP: no remote scripts, styles or connections (`connect-src` is
  limited to Tauri IPC). There is no updater and no telemetry.
- The pending redaction lives only in memory and is dropped when the popup
  closes.

## Develop

Requirements: Rust 1.85+, Node 20+ and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
cd apps/desktop
npm install
npm run tauri dev                              # hot reload
npm run tauri build -- --bundles app,dmg       # target/release/bundle/
```

Use `REDACTOR_CONFIG_DIR=/tmp/redactor-test npm run tauri dev` to try it
without touching your real config.

The desktop crate is excluded from the workspace's default members because it
needs platform webview libraries. Build it explicitly with
`cargo build -p redactor-desktop`.

## Layout

```
apps/desktop/
├── review.html, dashboard.html   # one page per window
├── src/
│   ├── lib/api.ts                # typed wrappers for the Rust commands
│   ├── lib/theme.css             # design tokens (light and dark)
│   ├── review/Review.svelte      # the popup
│   └── dashboard/                # settings, engagements, playground
└── src-tauri/src/
    ├── main.rs                   # builder, plugins, window lifecycle
    ├── flow.rs                   # clipboard → redact → review → clipboard
    ├── tray.rs                   # menu bar icon and menu
    ├── commands.rs               # IPC surface
    ├── state.rs, settings.rs     # shared state, app.toml preferences
    └── dto.rs                    # payloads for the webviews
```
