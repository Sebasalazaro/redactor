# Contributing

Thanks for helping make LLM-assisted pentesting safer.

## Ground rules

- **Never commit real data.** Fixtures and examples use fictional names (Globex
  Bank, Initech), `.example` / `.test` domains, RFC 5737 addresses
  (`192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`) and vendor
  documentation keys. Build realistic-looking tokens at runtime in tests
  instead of hardcoding them, so secret scanners stay quiet.
- **False negatives are worse than false positives.** When in doubt, mask.
- The core crate must stay offline: no networking dependencies.

## Workflow

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Changing redaction output? Regenerate the golden files and review the diff:

```sh
REDACTOR_BLESS=1 cargo test -p redactor-core --test golden
git diff crates/redactor-core/tests/expected
```

Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
(`feat(core): ...`, `fix(cli): ...`, `docs: ...`).
