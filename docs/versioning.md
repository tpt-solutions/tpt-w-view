# Versioning and changelog process

## Versions

- Versions follow [SemVer](https://semver.org/). The workspace version lives in the root
  `Cargo.toml` (`[workspace.package] version`), and `package.json` and `src-tauri/tauri.conf.json`
  must match it.
- Pre-1.0 (`0.x`): minor bumps may change behaviour. Patch bumps are fixes only.
- `1.0.0` is the first release that promises stability for the CLI flags, the JSON report
  schema and the exit codes documented in `docs/cli.md`.

## Changelog

- Every user-visible change adds a line under `## [Unreleased]` in `CHANGELOG.md`, in the right
  group (Added, Changed, Fixed, Removed, Security).
- On release, `[Unreleased]` becomes the new version heading with the date, and an empty
  `[Unreleased]` is added above it.
- Security fixes are listed under **Security** and reference the advisory or issue when there is one.
