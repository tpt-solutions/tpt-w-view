# Security review: path handling, IPC and the CLI

Scope: the batch/CLI path handling, the Tauri IPC boundary, and the Archon capability boundary.
This is a code-level review of the current tree, not an external audit.

## Trust model

- The **desktop UI** runs local TypeScript in a Tauri webview. Its content is bundled with the
  app; it does not load remote scripts (CSP in `src-tauri/tauri.conf.json`).
- Paths reach the Rust side from the **native file dialog**, the **launch argument**, or a
  **directory listing** the app itself produced. They are still validated, because the webview is
  the less-trusted side of the boundary.
- The **CLI** runs with the caller's own privileges. It operates on the paths it is given, the same
  as any command-line tool. Its checks guard against mistakes, not against the user.

## Path validation (IPC boundary)

Implemented in `src-tauri/src/ipc.rs`:

| Rule | Applies to |
|------|-----------|
| Must be absolute | every path |
| No `..` components | every path |
| No NUL bytes; not empty | every path |
| Must be an existing file with a supported image extension | image commands |
| Must be an existing directory | directory listing, watch, batch output folder |
| Output must have a supported output extension and an existing parent | save-as |

Covered by unit tests: `path_text_rules`, `image_and_directory_validation`,
`output_extension_and_parent_checked`, `batch_dto_rejects_traversal_and_bad_format`,
`launch_argument_selection_skips_bad_entries`.

**Known gap:** validation does not restrict paths to a scope such as the user's home or the
folders the user chose. The native dialog and the launch argument already limit what the UI
usually sends, but a compromised webview could request any absolute image path. A tighter fix is
to grant read scope per chosen folder through Tauri's filesystem scope.

**Decision (accepted for v1):** scope is not enforced. Enforcing it means registering the fs plugin
so the dialog records picks in the scope, then checking that scope in every path command. The
folder tree and navigation would also have to grant each folder the user opens, or their
thumbnails would fail. The webview can still list any folder it asks for, so the check would
only block paths the webview never named. That is a small gain for a large change that cannot be
tested without running the app. Revisit after the native flow has been tested on a real install.

## Overwrite safety

- Editing never writes the original. `EditSession::save` writes only to a path the caller gives,
  or to the original when `SaveTarget::Overwrite { confirmed: true }` is passed.
- The UI asks for confirmation before overwriting (`window.confirm`). The core engine enforces the
  flag independently, so the check does not depend on the UI.
- Batch jobs skip existing outputs unless `overwrite` is set (`existing_outputs_are_skipped_unless_overwrite`).

## Output writes

- Batch and save write through a temporary file and then rename it (`write_atomic`). A crash
  mid-write does not leave a truncated output under the final name.
- Rename patterns cannot produce path separators or empty names (`rename::render` rejects them).

## Crash reporting

- Off by default. The panic hook writes only when the user has enabled the marker file, and only
  to the app log directory (`src-tauri/src/diagnostics.rs`). No network path exists.

## Decoding untrusted files

- Corrupt, truncated and non-image input returns errors. Tests cover truncation, byte-flipping
  every byte of a PNG, and garbage input (`corrupted_bytes_never_panic`).
- RAW decoding relies on `rawler`. Garbage input returns an error (`garbage_bytes_are_an_error_not_a_panic`).
- A deterministic mutation pass (`core/tests/fuzz_lite.rs`) applies 4 000 seeded mutations
  (byte flips, truncation, corrupted length fields, inserted bytes) to JPEG, PNG and TIFF seeds
  with XMP/IPTC metadata. It checks probe, metadata, decode and metadata-preserving conversion.
  It is not coverage-guided: a libFuzzer or cargo-fuzz run on nightly is still outstanding.
- The `image` crate's default allocation limit (512 MB) bounds one decode.

## Memory budget

- Each batch job has a per-file working-memory budget (`BatchJob::memory_budget_bytes`, default
  1 GiB, `DEFAULT_MEMORY_BUDGET`). The estimate is the decoded RGBA8 size times two. Raster files
  are checked from their header (`probe`), HEIC files from their HEIF header, and RAW files from
  their sensor size (rawler's dummy decode), all before pixels are decoded. An over-budget file fails
  with a clear message and nothing is written. A zero budget is rejected.
- The RAW check has not been run against a real camera file: no sample is in the repository. The
  batch budget is also not a total across files, only a limit per file.

## Archon capability boundary

Not yet applicable: the capability-backed backend is not implemented. The path→block-id map never
reuses block ids, which prevents stale-handle reads (`released_ids_are_never_reused`). See
`docs/archon-design.md`.

## Open items

1. File-access scoping: accepted for v1, revisit after a manual test of the native flow (see the
   decision above).
2. Run a coverage-guided fuzzing pass on the decode entry points before v1. The deterministic
   mutation pass is in place.
3. ~~Add a per-job memory budget for batch conversion~~ Done as a per-file budget (see above).
   The RAW pre-check is in place; a real camera-file test is still needed.
