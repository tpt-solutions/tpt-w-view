# tpt-view-cli reference

`tpt-view-cli` runs the same batch engine as the desktop app, headlessly, for scripts and
automation.

## Synopsis

```text
tpt-view-cli convert <INPUT>... --out <DIR> [OPTIONS]
tpt-view-cli --help | --version
```

`INPUT` is one or more image files or directories. Directories are scanned for supported images,
non-recursively. Shell globs (`photos/*.jpg`) are expanded by your shell.

## Options

| Option | Default | Meaning |
|--------|---------|---------|
| `-o, --out <DIR>` | required | Output folder (created if missing) |
| `-f, --format <FMT>` | `jpg` | `jpg`, `png`, `webp`, `tiff`, `bmp` |
| `-q, --quality <1-100>` | `90` | JPEG quality. Ignored by lossless formats |
| `--max-edge <PX>` | — | Fit within a PX × PX box. Never enlarges |
| `--percent <P>` | — | Scale by P percent |
| `--size <WxH>` | — | Force exact dimensions (aspect ratio is not kept) |
| `-r, --rename <PATTERN>` | `{name}` | Output name pattern (see below) |
| `--start <N>` | `1` | First value of `{counter}` |
| `--preserve-exif` | off | Keep metadata from a JPEG source. JPEG → JPEG keeps EXIF, XMP and IPTC; JPEG → PNG and JPEG → WebP keep EXIF and XMP |
| `--overwrite` | off | Replace existing outputs instead of skipping them |
| `--json` | off | Print a JSON report on stdout instead of text |

The three resize options are mutually exclusive.

## Rename patterns

| Token | Output |
|-------|--------|
| `{counter}`, `{counter:N}` | Running counter, zero-padded to width N |
| `{name}`, `{name:lower}`, `{name:upper}` | Original file stem, optionally case-changed |
| `{date}` | EXIF capture time as `YYYYMMDD_HHMMSS`, or `unknown` |
| `{camera}` | EXIF camera model with path characters replaced, or `unknown` |

Write literal braces as `{{` and `}}`. A pattern that produces an empty name or a path separator
is rejected before any file is touched.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Every file converted or skipped on purpose |
| `1` | At least one file failed. The rest of the batch still ran |
| `2` | Usage error (bad or missing arguments) |
| `3` | Job-level error: output folder not creatable, input directory unreadable |
| `130` | Interrupted with Ctrl+C. Files not yet started are reported as cancelled |

Ctrl+C takes effect between files, so a file being written finishes first.

## JSON report

```json
{
  "summary": { "total": 2, "converted": 1, "skipped": 0, "failed": 1, "cancelled": 0 },
  "files": [
    { "input": "C:\\photos\\a.png", "status": "converted", "output": "C:\\out\\a.jpg" },
    { "input": "C:\\photos\\b.png", "status": "failed", "error": "decode error: ..." }
  ]
}
```

`status` is one of `converted`, `skipped` (with `reason`), `failed` (with `error`) or `cancelled`.

Each file has a working-memory budget of 1 GiB (estimated as twice the decoded RGBA8 size). An
image over the budget fails with an error and is not written. Raster, HEIC and RAW files are all
checked from their headers before decoding.
Field names are part of the 1.0 compatibility promise (see `docs/versioning.md`).

## Examples

```sh
# Thumbnails for a folder, 800 px box, renamed with a counter
tpt-view-cli convert ./photos --out ./web --format jpg -q 85 --max-edge 800 -r "web_{counter:03}"

# Keep EXIF while re-encoding JPEGs at half size
tpt-view-cli convert ./photos --out ./half --format jpg --percent 50 --preserve-exif

# Machine-readable run for a pipeline
tpt-view-cli convert ./raw --out ./dng-previews --format png --max-edge 1600 --json > report.json
```
