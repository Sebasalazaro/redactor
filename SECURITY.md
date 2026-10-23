# Security policy

`redactor` is a data-protection tool, so a detection gap (a value that should
have been redacted but wasn't) counts as a security issue.

## Supported versions

Only the latest release gets fixes. Pre-1.0, a fix ships as a new minor or
patch release; there are no backports.

## Reporting

Please open a private
[security advisory](https://github.com/Sebasalazaro/redactor/security/advisories/new)
instead of a public issue. Include a **fictional** input that reproduces the
leak. Never include real client data.

You should get an answer within a week. Once a fix is released, the advisory
is published with credit to the reporter, unless you prefer otherwise.

## Scope

- Values that survive redaction but should not.
- Masked output that still allows recovering the original value.
- Real values from the reversible-mode vault reaching disk, logs or anything
  other than the clipboard the user asked to restore to.
- Any code path that performs network access (the app and the CLI never do;
  only `scripts/fetch-model*` downloads the optional model, on demand).
- The desktop app's IPC surface: a webview reaching files, the clipboard or
  shortcuts other than through the app's own commands.

Out of scope: what the user does with text after copying it, and other
applications reading the clipboard while it holds unredacted text.

## Verifying releases

Release archives come with a `SHA256SUMS` file and a signed build
provenance attestation:

```sh
gh attestation verify redactor-v0.1.0-x86_64-pc-windows-msvc.zip --repo Sebasalazaro/redactor
```
