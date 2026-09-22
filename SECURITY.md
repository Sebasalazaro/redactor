# Security policy

`redactor` is a data-protection tool, so a detection gap (a value that should
have been redacted but wasn't) counts as a security issue.

## Reporting

Please open a private [security advisory](../../security/advisories/new)
instead of a public issue. Include a **fictional** input that reproduces the
leak. Never include real client data.

## Scope

- Values that survive redaction but should not.
- Masked output that still allows recovering the original value.
- Any code path that performs network access.
