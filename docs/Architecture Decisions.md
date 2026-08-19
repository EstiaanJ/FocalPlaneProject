---
aliases:
  - Architecture decisions
  - Decided architecture
  - Technical decisions
---

# Architecture decisions

This note records decisions made by the human owner after [[Project Audits#Project audit — 2026-08-19|the 2026-08-19 project audit]]. These are instructions, not suggestions. Agents must not silently replace them with a different architecture.

## One processing architecture

FocalPlane will have one image-processing architecture centred on FocalCore. FocalCurve and FocalPlot remain independently runnable experimental applications and visual test harnesses, but they must not grow independent production pipelines.

Before moving experimental work into the product:

1. Find duplicated image, colour, execution, and loading code across FocalCore, FocalCurve, and FocalPlot.
2. Compare the implementations rather than automatically preserving the first one written.
3. Keep or combine the implementation with the clearest semantics and strongest tests.
4. Move validated GUI-independent processing into FocalCore.
5. Keep reusable egui widgets and standalone harness behaviour outside FocalCore.

FocalCore must not depend on egui, eframe, file dialogs, or application layout.

## Shared file and metadata boundary

Create a project-owned `focal-io` library for shared file-boundary responsibilities:

- detecting and decoding supported image formats;
- reading ICC profiles and colour-space metadata through a proper colour-management implementation rather than name matching;
- reading and applying orientation exactly once;
- reporting source bit depth and metadata;
- handling export encoding and output profile tagging;
- reporting transparency to the calling application.

`focal-io` may produce FocalCore image types, so its dependency may point toward FocalCore. FocalCore must not depend on `focal-io`; the processing engine should be usable with pixels supplied by a CLI, tests, memory, or another decoder.

The application owns user interaction. `focal-io` must not open confirmation dialogs itself.

## Transparency

Alpha is outside the photographic processing model. FocalCore processes opaque RGB images.

If an input contains genuinely transparent pixels:

1. warn the user that transparency is unsupported;
2. ask for confirmation before changing the image;
3. if confirmed, flatten it simply in linear light onto black;
4. if declined, cancel the open operation without modifying the source.

The warning must say that the image will be flattened onto black. RGBA images whose alpha is fully opaque may be accepted without a warning. Hidden RGB in fully transparent pixels must not become visible accidentally.

This policy replaces the need to support semi-transparent pixels inside scopes and processing modules. It does not permit inconsistent alpha handling before the flattening boundary.

## Colour pipeline for the MVP

For the decoded-image MVP, the editable curve uses a canonical **Adobe RGB (1998) perceptual domain**. This is a deliberate temporary processing contract, not the final architecture for camera RAW files.

The MVP path is:

```text
decode and interpret the source profile
→ convert to linear Adobe RGB (1998) without premature output-gamut clipping
→ encode with the Adobe RGB (1998) transfer curve
→ apply the editable curve in bounded [0, 1] Adobe RGB values
→ decode the Adobe RGB transfer curve
→ perform later linear processing as defined by the pipeline
→ convert and gamut-map to output linear sRGB
→ apply the sRGB output transfer function
→ quantise, tag, and encode
```

All supported decoded inputs, including sRGB images, enter this same canonical curve domain. This makes a curve preset mean the same operation regardless of the source file's RGB primaries.

The Luma curve must use coefficients appropriate to the canonical Adobe RGB primaries. It is called **Luma**, not Luminance, because it is calculated from perceptually encoded RGB values.

Input conversion, curve evaluation, output conversion, gamut handling, and quantisation are separate stages with independently testable contracts. Do not clip to sRGB before the curve.

## Colour pipeline for the proper implementation

The proper camera-RAW implementation will use a wide-gamut, scene-referred working domain. That is the only acceptable long-term direction.

The exact working primaries, scene encoding, perceptual curve representation, handling of negative and above-one values, and display transform remain decisions for later experimentation. Do not treat the MVP's canonical Adobe RGB curve domain as the permanent RAW architecture.

Keep image contracts explicit enough that the MVP path can be replaced under a new pipeline version. Do not add speculative ACES, HDR, or graph machinery before the human owner settles the actual scene-referred path.

## Curve features

FocalCurve remains an experimental harness and may retain source for:

- Smooth interpolation;
- Linear interpolation;
- Bezier handles;
- derivative editing;
- other interaction experiments which are useful for comparison.

The first production FocalCore integration includes only:

- Smooth interpolation;
- Linked RGB mode;
- Luma mode;
- Per-channel RGB mode.

Do not port Linear, Bezier, or derivative editing into Focal Editor or production FocalCore yet. Preserve their experimental source in FocalCurve. Influence-radius curve editing is explicitly deferred until after MVP.

## Render execution contract

FocalCore rendering needs a shared execution context containing at least:

- a cooperative cancellation token;
- progress reporting;
- explicit `Preview` or `Export` render quality;
- an immutable pipeline snapshot.

Modules must check cancellation often enough that obsolete work normally stops within **150 ms** on the target system. This is a latency budget, not permission to block the GUI. The GUI thread never performs image processing.

Progress must describe the current request and must not allow progress from a stale request to replace the newest state. Preview and Export use the same processing meaning and module order; quality may select documented approximations or resolution.

## Saved edit state

Prototype sidecars store all editing parameters required to reproduce the edit. Adjustments are stored as **absolute values**, not offsets from preset values.

For example, if a preset supplies contrast `+20` and the photograph is changed to `+15`, the adjustment records `+15`, not `-5`.

Sidecars carry an exact pipeline/schema version. FocalPlane is in rapid development:

- do not implement migrations;
- do not promise that old development sidecars will remain usable;
- reject an unsupported version clearly rather than silently interpreting it with current semantics.

Whether an edit retains a live preset reference or embeds a frozen preset snapshot is still unresolved. Source-image identity is also unresolved. Do not flatten away the conceptual distinction between preset, absolute adjustments, and photo-specific edits while those choices remain open.

## Geometry

Crop is deferred. Do not design or implement crop, resize-as-edit, orientation controls, masks, or other local/geometry editing as part of the first vertical slice.

File orientation is not an edit control: it is interpreted once by `focal-io` at decode time so every application sees the same displayed image.

## Scope interaction

FocalPlot does not continuously scan the image while the pointer moves over a scope.

- Clicking the RYB vectorscope or CIE display selects a colour-space region and starts the reverse pixel search.
- Moving the pointer alone does not start another full-image search.
- A new left click replaces the selected colour-space region and starts a new search.
- Right-click cancels an active reverse search and clears the locked highlight.
- Reverse-search work uses the shared cancellation approach and rejects stale results.

The existing image-to-scope hover and spatial rectangle experiment may remain in FocalPlot unless a later product decision changes it.

## Preserved project boundaries

- Focal Editor remains usable without FocalLib.
- The MVP uses an opinionated ordered pipeline, not a DAG.
- No crop or local adjustments for the first vertical slice.
- The old editor is a GUI reference only.
- Focal Editor implementation follows the initial human GUI description in [[Focal-Editor Old GUI]]; subsequent consequential GUI changes still require human input.
- Agents must bring consequential colour, interaction, and architectural decisions back to the human owner.

See [[Clean Architecture Migration]] for the instructions which put these decisions into practice.
