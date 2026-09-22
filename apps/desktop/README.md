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
- **Menu bar:** redact now, switch the active engagement, open settings.
- **Settings dashboard:** hotkey, review mode, global rules, engagement
  profiles, and a playground to try rules live.
- **Shares its config with the CLI** (`~/.config/redactor`), so both apply the
  same rules.

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
