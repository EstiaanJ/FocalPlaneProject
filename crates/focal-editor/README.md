# Focal Editor

The first standalone Focal Editor GUI slice follows [`docs/Focal-Editor Old GUI.md`](../../docs/Focal-Editor%20Old%20GUI.md): open one PNG or JPEG directly, show Before and After previews, adjust exposure and contrast, and keep a histogram visible. It uses egui/eframe and performs image loading and preview rendering away from the UI thread.

Its layout follows the old editor's visual structure: Navigator at the upper left, presets beneath it, histograms and FocalPlot scopes at the top of the right rail, controls below them, the image viewer in the centre, and a filmstrip along the bottom. The top bar carries the loading indicator and status, with `FOCALPLANE` at the far right.

The rail widths and the Navigator, histogram, and filmstrip subpanel sizes can be adjusted with draggable splitters.

Run it with:

```text
cargo run -p focal-editor -- path/to/photo.jpg
```

The current prototype saves editable parameters, including crop and straightening, to a versioned JSON sidecar and exports an 8-bit sRGB PNG. PNG processing retains 16-bit source precision where present. Its decoder currently supplies FocalCore with the decoded sRGB contract; the shared `focal-io` boundary and embedded-profile conversion remain the next colour-management integration work.

Phase One crop controls are now present, including a non-destructive overlay, side and rotation handles, aspect-ratio locking, and full-resolution export application. The advanced curve editor, old response curve, and library workflow remain absent. FocalCore's numerical scope analysis is integrated with FocalPlot presentation and an RYB Log/Linear display toggle; the standalone harness retains its experimental interactions.

Phase One preview rendering now samples the visible full-resolution source region in the background, bounded by the physical preview area and a one-megapixel ceiling. Zoomed previews therefore resample source detail instead of enlarging a fixed proxy; small images remain at native size and use nearest-neighbour presentation. Export rendering is asynchronous, uses the untouched full-resolution source, and writes an embedded sRGB ICC profile.

PNG and JPEG decoding applies orientation once and interprets embedded ICC profiles with a colour-management engine. Profiled inputs are converted to the canonical encoded Adobe RGB input contract; unprofiled inputs retain the documented sRGB assumption. Production scope analysis runs through FocalCore with cooperative cancellation, while FocalPlot remains responsible for scope presentation.

See [`../../docs/Focal Editor & Focal Core.md`](../../docs/Focal%20Editor%20%26%20Focal%20Core.md) and [`AGENTS.md`](AGENTS.md) for the architectural and development constraints.
