# Focal Editor

Focal Editor is the standalone desktop editor described in `../../docs/MVP.md` and `../../docs/Focal Editor & Focal Core.md`.

## Original slice and approved Phase One work

- Open one PNG, JPEG, or TIFF directly; do not require FocalLib or an import/catalogue workflow.
- Show Before and After previews.
- Implement exposure in stops and contrast from `-100` to `+100`.
- Keep a small input/output histogram visible.
- Save editable state as a versioned JSON sidecar.
- Export an 8-bit sRGB PNG.
- Keep the UI responsive with background loading/rendering and latest-request-wins result handling.
- Keep the current high-level layout: Navigator at the upper left, presets below it, histograms at the top of the right rail, controls below the histograms, the photo viewer in the centre, and the filmstrip along the bottom.
- Keep the loading/progress treatment in the top bar; the status remains there
  and the `FOCALPLANE` all-caps title sits at the far right of the same top bar.
- Make the major rails and subpanels resizable: left/right rail widths, filmstrip height, Navigator height, and histogram-panel height.

The current MVP scope excludes the advanced curve editor and response-curve controls. Crop and FocalCore-backed scope presentation were subsequently approved and implemented in MVP Phase One; follow `../../docs/MVP.md` for the later approved scope.

## Boundaries

- Use egui and eframe for the GUI.
- Keep decoding and pixel conversion in small testable functions until the shared `focal-io` crate exists.
- Keep all image processing in FocalCore. Do not create an editor-specific processing pipeline.
- Do not make FocalCore depend on egui, eframe, or file dialogs.
- Use immutable preview requests and ignore stale results.
- Do not silently decide unresolved colour-management or saved-state semantics. Decoded inputs now enter FocalCore with explicit sRGB or canonical Adobe RGB contracts; shared file-boundary ownership still belongs in the planned `focal-io` crate.

## Quality

- Add unit tests for image conversion, histogram bins, sidecar round trips, and latest-result acceptance.
- Prefer readable, modular code over a large `app.rs`.
- Keep the UI thread free of image decoding and rendering.
- Use British English in user-facing text and documentation where practical.
