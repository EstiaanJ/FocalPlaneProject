# Better Plots

A standalone egui/eframe experiment for FocalPlane plotting tools.

Its canonical project name is **FocalPlot** and its eventual package/folder name is `focal-plot`. The current app remains a standalone harness as well as the proving ground for reusable scope widgets.

The current prototype places a tabbed colour scope on the left half of the window and a loaded image on the right. The default tab is a CIE 1931 xy chromaticity plot with a black grid and coloured spectral-locus outline; the RYB vectorscope remains available beside it. Both follow the visual research in `../../docs/Vectorscope Research.md`.

## Credits

The scopes are based on and inspired by darktable's scopes implementation, especially `src/libs/scopes/vectorscope.c` in darktable. The RYB hue arrangement follows darktable's interpretation of Lisa Craig Gossett and Baoquan Chen's *Paint Inspired Color Mixing and Compositing for Visualization*. The CIE 1931 spectral locus follows the standard 2° observer data used by the local darktable and vkdt references. Darktable's source also discusses the alternative computational RYB model by Junichi Sugita and Tokiichiro Takahashi.

This Rust implementation preserves that lineage while separating analysis from GUI rendering and adapting the presentation to FocalPlane.

## Run

```text
cargo run
```

Use **Open image…** to load a PNG or JPEG. Image decoding and vectorscope analysis run away from the UI thread. The initial scope analyses the decoded sRGB preview and is intentionally not presented as RAW or scene-referred data.

An image path may be supplied for repeatable visual testing:

```text
cargo run -- ../../test-image/pure_chroma.png
```

The prototype currently assumes decoded pixel values are sRGB. It does not yet interpret arbitrary embedded ICC profiles, and the UI labels that limitation explicitly.

## Interactive selection

- Hover the image to highlight the sampled colour on the vectorscope. The default sample is one source pixel.
- Highlights use the inverse of the sampled image/trace colour so they remain visible over any hue.
- Scroll over the image to grow or shrink the circular sample. The circle is drawn over the image and the scope updates with its colours.
- Turn on **Draw rectangle**, then drag to create a region of interest. Drag inside the region to move it, or drag a corner handle to resize it. The vectorscope then shows only the rectangle's colour information.
- Scope tabs and the selected tab's controls share one compact row at the top of the left panel. CIE 1931 is always linear; RYB also offers the darktable-style logarithmic radial scale.
- Click a colour in either scope to lock highlighting onto matching pixels in the image with inverse colours. A new click replaces the selection; right-click clears it or cancels an in-progress search. Scroll over the scope to widen or narrow that colour-family selection. This reverse interaction deliberately has no rectangle mode.
- Image and region analysis run on the background worker and stale requests are ignored.

The planned `focal-io` crate will eventually own decoding, profiles, orientation, metadata, alpha handling, and encoding. FocalPlot should analyse buffers with explicit colour-domain contracts rather than grow a competing file or processing pipeline.

## Code layout

- `src/app.rs` — egui layout and rendering.
- `src/loader.rs` — background image decoding and analysis requests.
- `src/vectorscope.rs` — GUI-independent RYB/CIE 1931 mapping, density analysis, bilinear trace generation, reverse highlighting, and numerical tests.
