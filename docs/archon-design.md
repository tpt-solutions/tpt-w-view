# Archon integration: design and scope

Status: scoped integration. The path→block-id layer is implemented; the capability-backed cache
is not yet wired (it needs the `tpt-archon-bridge` crate, which is not a dependency of this repo).

## What Archon provides today (verified)

Investigation of the `tpt-archon` workspace:

- `tpt-archon-bridge` has a real `UnifiedPageCache` / `CorePageCache<D: BlockDevice>` with
  zero-copy `map_read`, `map_write` and `unmap` over block ids.
- `Capability` and `Resource::Page` exist, and capabilities are minted by `CapabilityIssuer::mint`.
- There is **no filesystem or directory concept**. `Resource` has only `Page`, `Channel` and
  `Device` variants: no path, directory or file-metadata type.
- There is **no image decoding or thumbnailing code** anywhere in the workspace.
- There is **no GPU, rendering or texture capability**. The only "gpu" crate is an IR-emission
  layer for the SQL engine's vectorized scans, which is unrelated to rendering.

So the claim in `spec.txt` that Archon gives "instant, zero-allocation scrolling through
directories" through its page cache describes a goal, not an existing capability.

## What tpt-view builds itself

| Concern | Owner | Where |
|---------|-------|-------|
| Directory walk and watch | tpt-view | `core/src/scan.rs`, `core/src/watch.rs` |
| Image decode and thumbnails | tpt-view | `core/src/codec.rs`, `core/src/thumbs.rs` |
| Path → block-id mapping | tpt-view | `platform-archon/src/path_map.rs` (no Archon dependency) |
| Zero-copy byte cache for decoded pixels | Archon `UnifiedPageCache` (via capabilities) | `platform-archon` backend, behind the `archon` feature |

This mirrors how `tpt-vault` layers its own crypto engine over the same page cache: Archon
supplies storage primitives, and the application owns its semantics.

## Path → block-id mapping

`PathBlockMap` gives each file a contiguous run of block ids sized to its cached bytes:

- `allocate(path, byte_len, block_size)` reserves `ceil(byte_len / block_size)` blocks, minimum
  one, so every path has a handle, including empty files.
- `release(path)` frees the file's ids. **Ids are never reused.** A recycled id would let a stale
  handle read another file's cached bytes, which is the capability-confusion case the security
  review checks for.
- A file that grows gets a fresh range, and its old range is retired.

Ids are monotonic within one map. Persistence across restarts is not yet designed: the cache is
rebuilt at startup, so a restart starts with fresh ids.

## Feature gating

The `archon` Cargo feature (off by default) gates the capability-backed backend. Windows and
Linux builds never compile Archon code, and the path map is always available for testing.

## Proposed upstream primitives

These would let a later version integrate more deeply. None exist yet; they are proposals for the
Archon maintainers, not dependencies of this project.

1. **A directory- and file-aware `Resource` variant** (path identity, size, modification time),
   replacing the hand-rolled path→block-id layer with a first-class primitive.
2. **A GPU or texture resource** that lets decoded tiles live in device memory under the same
   capability model, so the wgpu renderer (`render/`) can read blocks directly.

## Test strategy

- Path-map behaviour is unit-tested in `platform-archon` without Archon (`cargo test -p
  tpt-view-archon-platform`).
- The capability backend will be tested against `tpt-archon-bridge`'s test capability issuer, so
  CI does not need a running Archon kernel. These tests are not written yet.

## Open questions

- Should `allocate` take ownership of a block-id range from Archon directly, rather than
  allocating ids itself? That depends on whether `CapabilityIssuer` can mint ranges.
- Eviction policy for the page cache under memory pressure. Not designed.
