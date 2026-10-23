# Releasing

Releases are built and published by
[`.github/workflows/release.yml`](../.github/workflows/release.yml) when a
`v*` tag is pushed. Nothing is built on a developer machine.

1. **Dry run.** Actions → Release → *Run workflow* on `main`. It builds
   every artifact without publishing; download them from the run and try
   the CLI and the desktop app on each platform you can.
2. **Version.** Bump `version` in the root `Cargo.toml` (the crates inherit
   it), `apps/desktop/package.json` and `apps/desktop/src-tauri/tauri.conf.json`
   if it sets one, then run `cargo check` so `Cargo.lock` follows.
3. **Changelog.** Rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and
   add a new empty `## [Unreleased]` above it. The release notes are this
   section; the workflow fails if it is missing.
4. **Merge** that as `chore: release vX.Y.Z` through a pull request.
5. **Tag** the merge commit and push the tag:

   ```sh
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```

The workflow attaches the CLI archives, the desktop installers, a
`SHA256SUMS` file and a build provenance attestation for each file.

## Signing

The desktop apps are not code-signed yet: macOS shows a Gatekeeper prompt
and Windows a SmartScreen prompt on first launch, and the release notes
explain how to get past them. Signing is planned through
[SignPath Foundation](https://signpath.org) (free for open source) for
Windows and an Apple Developer ID for macOS.
