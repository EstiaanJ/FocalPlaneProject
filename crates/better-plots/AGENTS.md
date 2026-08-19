# Better Plots

This sub-project develops standalone plotting and image-analysis experiments before useful components move into Focal Editor or Focal Core.

The canonical name is **FocalPlot** and the eventual Rust package/folder name is `focal-plot`. Keep it independently runnable as a visual test harness while designing the scopes as reusable widgets.

## Engineering standard

- Use Rust edition 2024, egui, and eframe.
- Keep analysis algorithms independent of egui and directly testable.
- Prefer small modules with explicit responsibilities and narrow interfaces.
- Prioritise correctness, readability, maintainability, and meaningful test coverage.
- Forbid unsafe Rust unless the human owner explicitly approves a concrete need.
- Do not silently settle consequential colour-science, plotting, interaction, or architectural choices. Present trade-offs to the human owner.
- Use human visual feedback as one testing vector alongside automated numerical tests.
- Use British English where practical.

## Vectorscope prototype

- Build a standalone application which loads PNG and JPEG images.
- Split the main content evenly: vectorscope on the left and fitted image preview on the right.
- Begin with the CIE 1931 xy scope as the default tab, with the RYB-style vectorscope available as a second tab; both are researched in `../../docs/Vectorscope Research.md`.
- Use a deep near-black background rather than darktable's grey background.
- Preserve the colourful, powder-like density trace and a restrained coloured hue ring.
- Keep image loading and vectorscope analysis off the UI thread.
- Hover sampling defaults to one source pixel. Scrolling over the image changes the circular sample radius and the circle must be visible over the image.
- The rectangle tool must support drawing, moving, and corner resizing. A committed rectangle replaces the full-image scope with its region-only analysis.
- Selection overlays should remain visually distinct from the base trace and use stale-result rejection when analysis requests overlap.
- Colour highlights should be the inverse of the colour being highlighted, including the image marker and hover trace.
- Scope tabs and controls for the selected tab share one row across the top of the scope panel. CIE 1931 is linear-only; RYB may use a logarithmic radial scale.
- The CIE 1931 background should show a smooth coloured spectral-locus outline over black and grid only; do not fill the horseshoe with translucent colour. The trace should retain the source image colours at plotted chromaticities.
- Dot sharpness and radial scale are presentation controls; keep them separate from the colour-space analysis.
- Clicking a colour in either scope locks reverse highlighting onto matching image pixels using inverse colours. A new click replaces the selection; right-click clears it or cancels an in-progress search. Scrolling over the scope adjusts the colour-search radius. This reverse interaction exists in both tabs and deliberately has no rectangle mode.
- Clearly separate image decoding, vectorscope analysis, visual texture generation, and egui layout.
- The current prototype may analyse the decoded sRGB image. Label that domain in the UI and do not imply that it represents unbounded RAW data.
- Move decoding, profile interpretation, orientation, metadata, alpha handling, and encoding to the planned shared `focal-io` boundary when it exists. Scope analysis must accept an explicit colour-domain contract rather than guessing from a buffer.
- Superseded analysis must observe cancellation within 150 ms under supported conditions, and stale work must never replace a newer accepted result.
- Keep the design open to other scope coordinate systems without implementing them speculatively.

## Reference implementation

The local darktable vectorscope implementation is available at:

`/home/estiaan/code/Reference_Projects/darktable-master/src/libs/scopes/vectorscope.c`

Use it as an algorithm and behaviour reference, but produce idiomatic Rust and correct defects or accidental constraints rather than transliterating C structure-for-structure.

Preserve visible credit to darktable and to the sources it cites for the adopted RYB model, including Gossett and Chen's *Paint Inspired Color Mixing and Compositing for Visualization*. Do not remove attribution during UI or documentation cleanup.
