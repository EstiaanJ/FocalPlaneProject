# FocalCurve

This is a small Rust/egui prototype for refining the exposure-curve interaction before it becomes a Focal Editor widget.

The app remains a standalone harness as well as the proving ground for a reusable widget.

## Run it

```text
cargo run -p focal-curve
```

The build script creates a deterministic 320 × 192, 16-bit RGB PNG with an embedded Adobe RGB (1998) ICC profile. It contains gradients and colour patches so curve inversions, clipping, and channel separation are easy to see. This is a bounded PNG fixture, not unbounded RAW scene data. Use **Open image…** to choose another PNG or JPEG from the GUI.

## Prototype behaviour

- Before and After previews are side by side with shared zoom and pan.
- The preview worker processes immutable snapshots on a background thread. A newer request cancels or abandons an older one.
- Curve dragging updates the graph immediately, but the After preview is rendered on pointer release.
- Linked RGB, Luma, and Per-channel RGB are alternatives for one curve stage; they are not stacked.
- Luma may retain explicit coefficient definitions for experimentation without silently changing the renderer.
- Right-click to add a control point on the current function, left-drag an anchor or visible Bezier handle, and middle-click an anchor to delete it or a handle to reset it. The first and last anchors are protected from deletion but can move horizontally; X positions remain strictly ordered. Interior Bezier handles remain collinear and opposite around their anchor, while their lengths remain independently adjustable.
- Smooth points use a safeguarded cubic interpolator that clamps each segment to its control-point interval; Linear and piecewise Bezier interpolation are also available for comparison. Bezier handle X positions are constrained so the curve remains a mathematical function.
- Smooth cubic mode exposes a per-point tension control: while dragging a point, scroll up or down to make the transition around that point looser or tighter. Linear mode does not use tension.
- Derivative mode edits the same underlying tone curve through an integrated `d(output) / d(input)` representation. Identity is a horizontal line at derivative 1; negative and greater-than-one slopes are allowed. The derivative graph uses the same point, handle, insertion, and deletion interactions.
- The curve graph shows Adobe RGB luma histograms on its axes: input in magenta along the bottom and output in cyan along the left. Output is hidden in derivative mode. Histogram calculation can use all decoded pixels or a bounded preview sample, and its 128-bin calculation is labelled approximate.
- PNG and JPEG inputs are supported at 8-bit and 16-bit source precision where the format provides it.
- The loader first attempts to identify sRGB or Adobe RGB from embedded ICC metadata, then checks EXIF `ColorSpace` and the PNG `sRGB` chunk. The detected choice is shown in the toolbar and can be overridden with the Input colour space menu. Images without recognised metadata default to sRGB.
- Export asks for a destination and writes the latest After image as an 8-bit PNG tagged with the sRGB colour space. Encoding and filesystem failures are shown in the UI.

## Pipeline contract

The CPU reference pipeline is deliberately ordered and kept outside the egui module:

1. Decode a PNG or JPEG and read its embedded ICC, EXIF, and standard PNG colour-space metadata.
2. Convert every supported input into the canonical Adobe RGB (1998) perceptual curve domain. sRGB input must not bypass this canonicalisation.
3. Apply exactly one selected curve mode in that Adobe RGB domain.
4. Decode the adjusted values to linear Adobe RGB, perform the output colour transform and gamut handling to sRGB, then encode sRGB.
5. Quantise the result to an 8-bit sRGB-tagged PNG for preview/export.

This is the intended MVP contract, not a description of every part of the current implementation. The fixed matrix and gamma are a readable prototype reference rather than complete ICC colour management. Shared decode/profile/encode work belongs in the planned `focal-io` boundary, while processing semantics belong in FocalCore. A proper RAW implementation will replace this bounded MVP domain with a wide-gamut scene-referred working domain.

Smooth interpolation with Linked RGB, Luma, and Per-channel RGB is the production target. Linear, piecewise Bezier, and derivative-source editing remain experimental FocalCurve facilities. Influence-radius interaction is deferred until after the MVP.

## Code layout

- `src/curve.rs` — GUI-independent curve points, interpolation, handle/tension behaviour, and modes.
- `src/pipeline.rs` — PNG/JPEG and metadata handling, colour-domain conversion, histograms, rendering, and output encoding.
- `src/loader.rs` — background image loading and colour-space re-preparation requests.
- `src/preview.rs` — latest-request-wins worker and progress events.
- `src/app.rs` — egui layout and interaction only.
- `build.rs` — deterministic 16-bit PNG fixture generation.
- `assets/adobe_rgb_1998.icc` — small embedded A98/Adobe RGB ICC profile used by the fixture.

The numerical tests are intentionally small and controlled. Visual judgement of the curve interaction remains part of the prototype workflow.
