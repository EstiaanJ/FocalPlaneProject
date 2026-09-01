# Focal Editor

The standalone Focal Editor GUI opens one PNG, JPEG, or TIFF directly, shows Before and After previews, exposes global adjustments, and keeps a histogram visible. It uses egui/eframe and performs image loading and preview rendering away from the UI thread. The current scope is recorded in [`docs/MVP.md`](../../docs/MVP.md) and [`docs/Focal Editor & Focal Core.md`](../../docs/Focal%20Editor%20%26%20Focal%20Core.md).

Its layout has scopes and presets in the left rail, foldable controls in the right rail, the image viewer in the centre, and a filmstrip along the bottom. The Navigator has been removed from the GUI. The top bar carries the loading indicator and status, with `FOCALPLANE` inset from the far right edge.

The left and right rail widths and filmstrip height can be adjusted with draggable splitters.

Run it with:

```text
cargo run -p focal-editor -- path/to/photo.jpg
```

The current prototype saves editable parameters, including crop and straightening, to a versioned JSON sidecar and exports 8-bit sRGB PNG or JPEG through one export dialog. JPEG quality is selectable, and the dialog can reuse the complete settings from the last successful export in the current session. PNG remains lossless and processing retains 16-bit source precision where present. Full-resolution export uses the Optimized executor (GPU when the complete snapshot is supported, otherwise its parity-tested multithreaded CPU path), while the CPU Reference remains the correctness oracle. Its decoder supplies FocalCore with explicit sRGB or canonical Adobe RGB contracts; moving that file and colour-management work into the shared `focal-io` boundary remains outstanding.

Phase One crop controls are now present, including a non-destructive overlay, side and rotation handles, aspect-ratio locking, and full-resolution export application. The advanced curve editor, old response curve, and library workflow remain absent. FocalCore's numerical scope analysis is integrated with FocalPlot presentation and an RYB Log/Linear display toggle; the standalone harness retains its experimental interactions.

Phase One preview rendering now samples the visible full-resolution source region in the background, bounded by the physical preview area and a one-megapixel ceiling. Zoomed previews therefore resample source detail instead of enlarging a fixed proxy; small images remain at native size and use nearest-neighbour presentation. Export rendering is asynchronous, uses the untouched full-resolution source, and writes an embedded sRGB ICC profile.

The first Phase Two interaction slice adds independent highlight and lowlight clipping overlays, a cursor-centred loupe toggled with `L`, and a fixed processing bar at the bottom of the right rail. Edits can be copied and pasted between images during the current session, and the white-balance picker derives warmth and tint from a clicked source pixel. TIFF input is supported alongside PNG and JPEG. Fujifilm X-T5 RAF files open through `focal-io` using the versioned, X-Trans-aware Camera-Neutral v4 baseline before editable adjustments; other camera RAW formats and HEIC remain unsupported.

PNG, JPEG, and TIFF decoding applies orientation once and interprets embedded ICC profiles with a colour-management engine. Profiled inputs are converted to the canonical encoded Adobe RGB input contract; unprofiled inputs retain the documented sRGB assumption. Production scope analysis runs through FocalCore with cooperative cancellation, while FocalPlot remains responsible for scope presentation.

See [`../../docs/Focal Editor & Focal Core.md`](../../docs/Focal%20Editor%20%26%20Focal%20Core.md) and [`AGENTS.md`](AGENTS.md) for the architectural and development constraints.
