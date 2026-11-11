# tpt-view user guide

## Browsing

- **Open a folder** with the toolbar button or Ctrl+Shift+O. The path appears as breadcrumbs above
  the grid; click any part to jump there.
- **Thumbnails** load as you scroll. They are cached on disk, so reopening a folder is fast.
- **Select** with click, Ctrl+click (toggle) and Shift+click (range). Ctrl+A selects all.
- **Open** an image by double-clicking it or pressing Enter.
- **Go up** one folder with Backspace, or click a breadcrumb.
- **Folder tree:** in the browse view, a tree on the left shows the folders from the drive root down to
  the open folder. Click a name to open it, or the arrow to expand or collapse it. The toolbar
  button Folders hides the tree. Folders that cannot be read show "Not available".

## Viewing

| Key | Action |
|-----|--------|
| ← / → , PageUp / PageDown, Space | Previous / next image |
| + / − | Zoom in / out |
| 0 | Fit to window (never enlarges) |
| 1 | Actual size (100%) |
| Drag, mouse wheel | Pan / zoom |
| S | Start or stop the slideshow (interval set in Settings, 4 seconds by default) |
| I | Show or hide the sidebar (EXIF and text info) |
| F | Fullscreen |
| Esc | Back to the grid |

## Editing

Press E (or R to rotate a quarter turn) to open the edit panel. Edits apply to a copy in memory;
the original file is not changed until you save.

- **Transform:** rotate left or right, flip horizontally or vertically, rotate by an angle, resize,
  crop by dragging a rectangle on the image or by typing the numbers. The Aspect dropdown
  (Free, 1:1, 4:3, 3:2, 16:9) constrains the drag.
- **Colour:** brightness, contrast, saturation, and levels with gamma. Sliders apply when released.
- **History:** Ctrl+Z to undo and Ctrl+Y (or Ctrl+Shift+Z) to redo. Up to 32 steps are kept.
- **Save as…** writes a new file. **Overwrite original…** replaces the file after a confirmation.

Closing the edit session with unsaved changes asks before discarding them.

## Batch convert

Press B (or the Batch button) with images selected. If nothing is selected, the whole folder is
used. Choose an output folder and format, optionally resize and rename, then Run. Progress is
shown per file, and Cancel stops after the current file. Results list each file's outcome.

Rename patterns use the same tokens as the CLI (see `docs/cli.md`). Presets are kept in Settings.

## Settings

Open Settings from the toolbar to change:

- default output format and JPEG quality;
- thumbnail size and rename presets;
- theme (follow system, light or dark);
- crash logging (off by default; reports stay on this computer);
- clearing the thumbnail cache.

## Camera RAW

RAW files open like other images. They are developed with default settings, so colours may differ
from the camera manufacturer's software. Per-file develop settings are not yet available.

## Troubleshooting

- **An image does not open:** the file may be corrupt or in an unsupported variant. The error
  appears in the sidebar. Enable crash logging only if the app itself crashes.
- **Thumbnails look stale:** they update when the file changes. Use Settings → Clear thumbnail cache
  to rebuild them all.
