# Focal Core

`focal-core` is the GUI-independent CPU reference pipeline for FocalPlane.
It currently establishes the image contract, serialisable module parameters,
immutable pipeline snapshots, the documented default order, and contract
checks at processing boundaries.

Rendering can use `Pipeline::render_with_context` with a cloneable
`CancellationToken`, typed stage progress, and explicit `Preview` or `Export`
quality. The quality modes currently have identical semantics until a preview
approximation is defined and approved. The compatibility `render` method uses
the export mode and discards progress.

The target architecture is documented in `../../docs/Architecture Decisions.md` and `../../docs/Clean Architecture Migration.md`. FocalCore owns the one production processing architecture; the planned `focal-io` crate owns decoding, profile interpretation, orientation, metadata, alpha handling, and encoding. Experimental applications must not become competing production pipelines.

The default processing order is:

1. input transform into linear Adobe RGB;
2. orientation and confirmed crop;
3. white balance;
4. exposure;
5. highlight and shadow processing;
6. decoded-image noise reduction;
7. contrast;
8. tonal curve;
9. local contrast;
10. saturation;
11. creative colour;
12. sharpening;
13. resize;
14. output transform to encoded sRGB;
15. quantisation and dithering.

Decode and encode are shared `focal-io` boundaries. They must produce and
consume images with explicit contracts; they are not pixel-processing modules
in the in-memory pipeline.

Input/output transforms, crop, warmth and tint, exposure, contrast, Smooth
tonal curves, local contrast, saturation, and decoded-image noise reduction
currently alter pixels. Highlight/shadow processing, creative colour,
sharpening, resize, and quantisation/dithering remain ordered identity
placeholders and are reported honestly as such.

The production curve evaluator is available as `SmoothCurve` and `CurveSet`; it
supports only Smooth interpolation with Linked RGB, Luma, and Per-channel RGB
modes in the canonical encoded Adobe RGB (1998) curve domain. Smooth uses the
safeguarded default tangent semantics; FocalCurve's user-adjustable tension
and handle controls remain experimental.

Validated GUI-independent vectorscope analysis is also available under
`focal_core::scope`. It owns numeric scope coordinates, decoded linear
RGB-to-scope analysis, density data, radial display mapping, and
reverse-selection overlays;
FocalPlot retains egui texture construction and drawing.

## Remaining decisions and work

- The proper RAW implementation will replace the MVP's Adobe RGB contract with
  a wide-gamut scene-referred domain whose exact space and encoding remain open.
- The permanent gamut-mapping algorithm remains a human-owned decision.
- Highlight/shadow processing, creative colour, sharpening, resize, and
  quantisation/dithering still need approved algorithms and parameter contracts.

Run checks from this directory with:

```sh
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
```
