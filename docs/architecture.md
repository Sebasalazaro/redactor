# Architecture

```
crates/
├── redactor-core/        # the engine: no I/O besides reading config files
│   ├── src/
│   │   ├── lib.rs        # public API re-exports
│   │   ├── config.rs     # global + engagement TOML model
│   │   ├── redactor.rs   # Redactor: pipeline, masking helpers, overlap resolution
│   │   ├── format.rs     # input format detection, Burp base64 normalization
│   │   ├── mask.rs       # pure masking primitives (cut_tail, keep_head, secret, ipv4, card)
│   │   ├── dictionary.rs # Aho-Corasick matcher for client names, users, hosts, terms
│   │   ├── jwt.rs        # JWT decoding and claim-aware redaction
│   │   ├── tld.rs        # TLDs accepted by the host detector
│   │   ├── finding.rs    # Category (with priority) and Finding
│   │   ├── store.rs      # config dir layout shared by the CLI and the app
│   │   └── detectors/
│   │       ├── patterns.rs  # format-independent regexes
│   │       ├── http.rs      # headers, cookies, authorization schemes, curl flags
│   │       └── keys.rs      # JSON / HAR / Postman / query / form, classified by key
│   └── tests/            # golden fixtures, leak and determinism tests
├── redactor-cli/         # `redactor` binary: stdin, files, clipboard
└── redactor-ner/         # optional GLiNER inference (ONNX Runtime) + worker protocol
apps/
└── desktop/              # Tauri menu bar app (Svelte UI + Rust shell)
```

`redactor-core::Store` owns the config directory, so the CLI and the desktop
app always read and write the same files. `Redaction::segments()` splits a
result into plain text and findings, which is what the review popup renders
and lets the user toggle.

## Pipeline

1. **Detect format.** `InputFormat::detect` looks at the first bytes. The
   format is informational, except for Burp exports, which get normalized.
2. **Normalize.** Burp stores requests as base64. They are decoded in place
   and marked `base64="false"`, so the LLM gets readable (redacted) traffic.
3. **Detect.** Every detector scans the whole text and pushes candidate
   `Finding`s (`start..end`, category, replacement). Detectors never edit the
   text, so they can't interfere with each other.
4. **Resolve.** Candidates are sorted by category priority (the declaration
   order of `Category`), then by length, and accepted greedily if they don't
   overlap an accepted span. A `BTreeMap` keyed by start offset keeps this
   `O(n log n)`.
5. **Apply.** Replacements are spliced in a single pass.

## Design decisions

**Deterministic masking instead of random pseudonyms.** The same value always
produces the same output, with no state to keep. The LLM can correlate ids
across requests, which matters for IDOR/BOLA analysis. The trade-off is that
two different values can collide (`1042` and `1043` both become `104*`).

**Client names are protected inside every other masker.** Hosts, secrets and
personal data all pass through `Dictionary::mask_around_clients`, so a
partially visible value never shows the client (`gbx-signing-2***` is fine,
`globex-prod-…` is not).

**Nested redaction.** JWT claims are fed back into the full engine
(`Redactor::redact_nested`), so an `iss` URL gets host masking and an email
claim gets email masking. Recursion is capped (`MAX_DEPTH`); anything deeper
is treated as an opaque secret.

**Quote-free JWT rendering.** Decoded tokens are printed as
`header{alg=RS256} payload{...}` rather than JSON, so they can be embedded in
JSON strings, curl arguments or headers without breaking quoting.

**Conservative TLD list.** The host detector only accepts a curated list of
TLDs. Extensions like `.py`, `.sh`, `.md`, `.zip`, `.id` and `.at` are
excluded, because `app.py` or `user.id` in a stack trace are far more common
than domains under those TLDs.

## Adding a detector

1. Add a function to the relevant module in `detectors/` (or a new module
   wired into `detectors::scan`).
2. Push findings with `scan.push(...)`, or `scan.token(...)` for opaque
   credentials, which decodes JWTs automatically.
3. Pick the category deliberately: its position in `Category` decides who wins
   on overlap.
4. Add a fictional fixture to `tests/fixtures`, plant the sensitive values in
   `PLANTED`, and bless the golden output.

## AI deep scan

`redactor-ner` runs token-level GLiNER models with ONNX Runtime and the
`tokenizers` crate, built without their HTTP features. It mirrors GLiNER's
Python processor: the prompt `<<ENT>> label ... <<SEP>>` precedes the words
(split with GLiNER's `\w+(?:[-_]\w+)*|\S` pattern), `words_mask` marks the
first sub-token of each word, and logits `[batch, words, labels, 3]` give the
start, end and inside probability of every word for every label. Spans are
decoded, then flattened greedily by score. Long inputs are scanned in
overlapping windows of 256 words.

ONNX Runtime keeps most of the memory it used after a session is dropped,
so the desktop app never loads the model in its own process. It starts a
worker (its own executable with `--ner-worker`) that speaks one JSON object
per line over stdin/stdout, and ends it when idle. The popup sends the text
as it would be copied; entities come back with UTF-16 offsets and are applied
only inside text that is not already redacted.
