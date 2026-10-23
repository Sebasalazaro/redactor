
## Install

Check any download against `SHA256SUMS`, and its build provenance with
`gh attestation verify <file> --repo Sebasalazaro/redactor`.

**CLI:** download the archive for your platform, unpack it and put
`redactor` on your `PATH`.

**Desktop (macOS, Apple Silicon):** open the `.dmg` and drag redactor to
Applications. The app is not signed with an Apple Developer ID yet, so the
first time right-click it and choose *Open*, or run
`xattr -dr com.apple.quarantine /Applications/redactor.app`.

**AI deep scan (optional):** run `scripts/fetch-model.sh` from the
repository to install the local model.
