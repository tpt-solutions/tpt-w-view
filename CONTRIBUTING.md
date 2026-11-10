# Contributing to tpt-view

tpt-view is dual-licensed under **MIT OR Apache-2.0** (see `LICENSE-MIT` and `LICENSE-APACHE`).
By contributing you agree that your work is released under the same terms.

## Source file headers (SPDX)

Every source file carries a two-line SPDX header at the top:

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions
```

For TypeScript/CSS use the same `//` or `/* */` form. For HTML use an HTML comment after
`<!doctype html>`. Files that cannot carry comments (JSON, lockfiles) are exempt.

## Checks before opening a PR

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm ci && npm run build
```

Tests must not depend on binary fixtures committed to the repository; generate them in code.
