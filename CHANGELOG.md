# Changelog

## [0.2.0]

### Added

- **Server-side window chrome on the canvas.** Every window now gets a frame and a title strip
  drawn by the compositor (in z-order, above the client surface, below the shell): amber border for
  the focused window, dim for the rest, the client title (or `app_id`), the logical size, and the
  camera zoom when it is not 1:1. The strip is cached per window and re-rasterised only when the
  title, focus or width changes.
- **Drag by the title strip.** The strip sits above the window rectangle, so the hit test did not
  see it and pressing it panned the canvas. A shifted hit test fixes it: the title now moves its
  window, and a double click on the title still runs fit-window.
- `phosphor::chrome` with three unit tests (text fitting by measured width, rendering, focus
  makes a difference).

### Fixed

- Clippy is clean across every product crate again (`tad-core`, `de-common`, `phosphor`,
  `compositor`): three `to_vec` allocations, a `next()` naming lint, a dead helper. Previous claim
  of "0 warnings" had drifted.

## [0.1.0]

First snapshot: infinite canvas, Wayland clients, X11 via xwayland-satellite, phosphor shell,
screencopy, DRM layers 0-1 (layer 1 written, unverified).
