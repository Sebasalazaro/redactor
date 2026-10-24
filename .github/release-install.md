
## Install

Check any download against `SHA256SUMS`, and its build provenance with
`gh attestation verify <file> --repo Sebasalazaro/redactor`.

**CLI:** download the archive for your platform, unpack it and put
`redactor` on your `PATH`.

**Desktop (macOS, Apple Silicon):** open the `.dmg` and drag redactor to
Applications. The app is not signed with an Apple Developer ID yet, so the
first time right-click it and choose *Open*, or run
`xattr -dr com.apple.quarantine /Applications/redactor.app`.

**Desktop (Windows, x64):** run the `-setup.exe`. It installs for the
current user only, without admin rights. It is not code-signed yet, so
SmartScreen may warn the first time: choose *More info* → *Run anyway*.

**AI deep scan (optional):** from the repository, run
`scripts/fetch-model.sh` (macOS, Linux) or
`powershell -ExecutionPolicy Bypass -File scripts\fetch-model.ps1` (Windows)
to install the local model.
