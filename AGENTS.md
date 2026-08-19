# FocalPlane repository instructions

Read these documents before making architectural or product changes:

1. `docs/README.md`
2. `docs/FocalPlane.md`
3. `docs/Architecture Decisions.md`
4. `docs/Clean Architecture Migration.md`
5. `docs/Engineering Principles.md`
6. `docs/Testing.md`
7. `docs/Bug Report.md`

## Non-negotiable direction

- This repository is a rewrite. `OLD_EDITOR` is a GUI reference only; do not carry over its film-stock model or processing pipeline.
- FocalCore is the single production processing architecture.
- FocalCurve and FocalPlot remain independent visual harnesses, but validated GUI-independent processing moves into FocalCore.
- Shared decoding, ICC, metadata, orientation, transparency-boundary, and encoding work belongs in the planned `focal-io` library.
- FocalCore must not depend on egui, eframe, file dialogs, or application layout.
- Use an explicit ordered pipeline. Do not introduce a DAG or node architecture.
- Do not implement crop or local adjustments for the first vertical slice.
- Do not begin Focal Editor implementation until the human owner provides and documents the GUI description.

## Current colour decisions

- The decoded-image MVP curve domain is canonical encoded Adobe RGB (1998).
- Convert and gamut-map to output sRGB after the editable curve, never before it.
- The proper RAW implementation will use a wide-gamut scene-referred working domain; its exact space and encoding remain human-owned decisions.
- Production curve integration initially includes Smooth interpolation with Linked RGB, Luma, and Per-channel RGB only.
- Keep Linear, Bezier, derivative, and other research modes in FocalCurve rather than porting them into Focal Editor.

## Engineering standard

- Prioritise correctness, readability, maintainability, and meaningful test coverage.
- Use Rust, egui, and eframe for desktop GUI work.
- Forbid unsafe Rust unless the human owner explicitly approves a concrete need.
- Keep processing independent of GUI types and directly testable.
- Validate versions, contracts, dimensions, and parameters at boundaries.
- Use immutable render snapshots with cooperative cancellation, progress, and explicit Preview/Export quality.
- Obsolete processing should normally stop within 150 ms on the target system.
- Treat human visual feedback as a required testing vector for visible behaviour.
- Do not make consequential product, interaction, colour-science, or architectural decisions silently.
- Use British English where practical.

## Known bugs

Known-bug tests in `docs/Bug Report.md` are deliberately ignored by the normal suite. Fix the implementation before removing an ignore, then retain the test as an active regression test.

Run routine checks from the workspace root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
