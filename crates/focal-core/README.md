# Focal Core

`focal-core` is the GUI-independent CPU reference pipeline for FocalPlane.
It currently establishes the image contract, serialisable module parameters,
immutable pipeline snapshots, the documented default order, and contract
checks at processing boundaries.

The cleaner target architecture is documented in `../../docs/Architecture Decisions.md` and `../../docs/Clean Architecture Migration.md`. FocalCore owns the one production processing architecture; the planned `focal-io` crate owns decoding, profile interpretation, orientation, metadata, alpha handling, output conversion, and encoding. Experimental applications must not become competing production pipelines.

The default processing order is:

1. input transform (sRGB transfer-function decoding for the prototype);
2. orientation;
3. white balance;
4. exposure;
5. highlight and shadow processing;
6. contrast;
7. tonal curve;
8. creative colour;
9. noise reduction;
10. sharpening;
11. resize;
12. output transform;
13. quantisation and dithering.

Decode and encode are shared `focal-io` boundaries. They must produce and
consume images with explicit contracts; they are not pixel-processing modules
in the in-memory pipeline. Crop is deferred for now.

Only the sRGB input/output transforms, RGB multiplier form of white balance,
exposure, and a provisional contrast operation currently alter pixels. The
remaining modules are ordered identity placeholders. In particular, the tonal
curve slot is ready to receive the separately developed curve evaluator
without coupling Focal Core to its experimental GUI.

## Current implementation versus decided direction

- The serialised pipeline configuration currently supports only linear sRGB.
  This is existing prototype state, not the chosen MVP contract. MVP inputs
  canonicalise through Adobe RGB (1998), with the editable curve operating in
  its perceptual domain. The proper RAW implementation will use a wide-gamut
  scene-referred working domain; its exact space and encoding remain open.
- Contrast is provisionally linear around 18% grey. Its photographic meaning
  and response need visual comparison before this becomes a locked pipeline
  definition.
- White balance accepts explicit RGB multipliers. Mapping the editor's warmth
  and tint controls to these multipliers remains a colour-science decision.
- Orientation, highlight/shadow processing, curves, creative colour,
  noise reduction, sharpening, resize, quantisation, and dithering still need
  algorithms and parameter contracts.

Run checks from this directory with:

```sh
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
```
