# redactor

**Offline redaction for pentest traffic before it goes to an LLM.**

Pasting captured requests into an AI assistant is a great way to speed up a
security test, and a great way to leak a client's hosts, session cookies and
customer data. `redactor` rewrites the sensitive parts of HTTP traffic
**locally**, while keeping enough structure for the model to still reason
about the request: token types, JWT claims, id relationships, internal vs.
external IPs.

[![CI](../../actions/workflows/ci.yml/badge.svg)](../../actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

```http
POST /api/v2/accounts/88127/transfers HTTP/1.1
Host: api.globexbank.example
Authorization: Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Nzby5nbG9iZXhi...
Cookie: GBX_SESSION=a7f3e9c1d5b2084f6e1a9c3d7b5f2e80
X-Forwarded-For: 203.0.113.45

{"toAccountId": "99301", "amount": 1500, "email": "maria.gomez@globexbank.example", "otp": "482913"}
```

becomes

```http
POST /api/v2/accounts/8812*/transfers HTTP/1.1
Host: api.[CLIENT].example
Authorization: Bearer [JWT header{alg=RS256, typ=JWT} payload{iss=https://sso.[CLIENT].example, sub=8812*, name=Ju** Pé***, roles=[customer], exp=1790000000} sig=…[len=12]]
Cookie: GBX_SESSION=a7f3e9c1d…[len=32]
X-Forwarded-For: 203.*.*.*

{"toAccountId": "9930*", "amount": 1500, "email": "ma***.go***@[CLIENT].example", "otp": "…[len=6]"}
```

The account id `88127` shows up as `8812*` both in the path and in the JWT
`sub` claim. Masking is deterministic, so the model can still tell that the
token belongs to the account being modified.

## Features

- **100 % offline.** No network access, no telemetry. The engine is a pure
  Rust library.
- **Understands the formats you actually paste:** raw HTTP requests and
  responses, `curl` and `fetch()` snippets, HAR files, Burp XML exports
  (base64 bodies are decoded), Postman collections, tool output and logs.
- **Partial, consistent masking** that keeps context:

  | Data | Example | Rule |
  |---|---|---|
  | Client name, any spelling | `GlobexBank`, `globex-bank` → `[CLIENT]` | dictionary, case-insensitive |
  | Hosts | `portal.initech.com.co` → `portal.ini****.com.co` | registrable label partially hidden |
  | JWT | decoded, claims masked, signature dropped | `alg`, roles, scopes, `exp` kept |
  | Secrets (cookies, API keys, passwords) | `a7f3e9c1d…[len=32]` | ~30 % prefix + length |
  | Identifiers | `834512` → `8345**`, UUID tail hidden | ~20 % cut |
  | Personal data | `Juan Pérez` → `Ju** Pé***` | head of each word |
  | IPv4 | `10.12.4.21` → `10.12.*.*`, `203.0.113.45` → `203.*.*.*` | keeps internal/external hint |
  | Cards | `4111 **** **** 1111` | Luhn-validated |
  | Private keys, vendor tokens | AWS, GitHub, GitLab, Stripe, Slack, Google... | always |

- **Global config + per-engagement profiles.** The client's name lives in one
  place, while hosts, test users and device names change per test.
- **Tested against leaks.** Every fixture has planted secrets, and the test
  suite asserts that none of them survive redaction.
- **Optional AI deep scan.** A local GLiNER model finds what patterns cannot
  (a person's or company's name in free text). It runs on demand, in a
  separate process that exits when idle, and is never downloaded by the app.

## Desktop app

A menu bar app for everyday use: copy traffic anywhere, press **⌥⌘R**, check
the colored diff, press **Enter** and paste. Click any value to reveal or
re-mask it, or select text and press **M** to mask something the engine missed.
Engagements switch from the menu bar, and a dashboard shows the last
redaction, live memory use and the effective rules of each engagement, over
the same config files the CLI uses. Idle, it uses about 70 MB.

![Desktop flow: copy traffic, press the hotkey, review, paste](docs/desktop-flow.svg)

See [apps/desktop](apps/desktop) to build it.

### AI deep scan (optional)

```sh
scripts/fetch-model.sh   # 49 MB, pinned revision, SHA-256 verified
```

Downloads `knowledgator/gliner-pii-edge-v1.0` (Apache-2.0) into
`~/.config/redactor/models/`. The review popup then offers **Deep scan (AI)**.
The model is English-focused, and ONNX Runtime is linked into the app at
build time.

## Install (CLI)

Requires Rust 1.85+.

```sh
git clone https://github.com/<you>/redactor && cd redactor
cargo install --path crates/redactor-cli
redactor --init          # writes ~/.config/redactor/config.toml + an example engagement
```

## Usage

```sh
pbpaste | redactor | pbcopy                 # pipe (macOS)
redactor --clipboard -e globex-q3           # rewrite the clipboard in place
redactor request.txt --report               # summary on stderr
redactor export.har --json > findings.json  # findings without original values
```

`-e` takes either a path or the name of a file in
`~/.config/redactor/engagements/`. Set `REDACTOR_CONFIG_DIR` to use another
directory.

## Configuration

```toml
# ~/.config/redactor/config.toml (global)
client = ["Globex Bank", "Globex"]
allow_domains = ["cdn.jsdelivr.net"]

[[terms]]
value = "MacBook-de-Pentester"
replacement = "[DEVICE]"

[masking]
id_cut = 0.2       # hide 20 % of identifiers
secret_keep = 0.3  # show a 30 % prefix of secrets (max 12 chars)
pii_keep = 0.4     # show 40 % of each word of personal data
```

```toml
# ~/.config/redactor/engagements/globex-q3.toml
name = "globex-webapp-2026q3"
hosts = ["srv-db01", "jenkins-int"]   # internal names that are not FQDNs
users = ["qa.tester01"]
sensitive_headers = ["X-Globex-Device"]
sensitive_keys = ["deviceFingerprint"]
```

See [`examples/`](examples) for commented versions.

## How it works

```mermaid
flowchart LR
    A[input] --> B[detect format]
    B --> C[normalize<br/>decode Burp base64]
    C --> D{detectors}
    D --> D1[patterns<br/>JWT, keys, emails, IPs, hosts, ids]
    D --> D2[HTTP<br/>headers, cookies, auth schemes, curl]
    D --> D3[keys<br/>JSON, HAR, Postman, query, form]
    D --> D4[dictionary<br/>client, users, hosts, terms]
    D1 & D2 & D3 & D4 --> E[resolve overlaps<br/>priority, then length]
    E --> F[apply] --> G[redacted output + findings]
```

Detectors only propose findings. A single resolver keeps the best
non-overlapping set, so adding a detector never corrupts the output. More in
[docs/architecture.md](docs/architecture.md).

## Security model

- `redactor` reduces risk; it does not remove the need to look at what you
  paste. Always review the output. The desktop app (see roadmap) will make
  that review a single keystroke.
- Everything runs locally. The core crate has no networking dependencies, and
  the desktop app has a strict CSP and exposes no plugins to its webviews.
- Findings serialized with `--json` never include original values.
- Partially masked values are designed to be unusable (for example, secrets
  never show more than 12 characters), but they are not encryption.

Report vulnerabilities as described in [SECURITY.md](SECURITY.md).

## Roadmap

- [x] **Core engine and CLI:** detectors, masking, configs, golden and leak tests
- [x] **Desktop app (Tauri):** global hotkey, review popup with a diff, menu bar, settings dashboard
- [x] **Local NER (GLiNER, ONNX):** on-demand deep scan of names, organizations, usernames, addresses, fully offline
- [ ] **Multilingual model:** span-level GLiNER models (e.g. `gliner_multi_pii-v1`) for Spanish and other languages
- [ ] **Reversible mode:** per-engagement mapping encrypted with AES-GCM, key stored in the macOS Keychain, to restore real values in LLM answers
- [ ] **Optional encrypted history** of redactions

## Development

```sh
cargo test                                             # everything
REDACTOR_BLESS=1 cargo test -p redactor-core --test golden  # accept new golden outputs
cargo clippy --all-targets && cargo fmt --check
```

All fixtures are fictional: Globex Bank, `.example` domains, RFC 5737 IP
addresses and AWS documentation keys. Please keep it that way. See
[CONTRIBUTING.md](CONTRIBUTING.md).

## Disclaimer

Built for authorized security testing. Follow your engagement's rules on
handling client data, including whether it may be shared with third-party AI
services at all.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option.
