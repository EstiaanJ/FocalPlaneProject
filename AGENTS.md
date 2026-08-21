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

- This repository is a rewrite. Keep the production processing architecture centred on FocalCore; do not introduce a separate film-stock pipeline.
- FocalCore is the single production processing architecture.
- FocalCurve and FocalPlot remain independent visual harnesses, but validated GUI-independent processing moves into FocalCore.
- Shared decoding, ICC, metadata, orientation, transparency-boundary, and encoding work belongs in the planned `focal-io` library.
- FocalCore must not depend on egui, eframe, file dialogs, or application layout.
- Use an explicit ordered pipeline. Do not introduce a DAG or node architecture.
- Crop was excluded from the first vertical slice and is now approved for MVP Phase One. Do not add local adjustments.
- Keep GUI scope aligned with the current product, MVP, and interaction documentation; do not make consequential product decisions silently.

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

Confirmed defects and their active regression tests are recorded in `docs/Bug Report.md`. Fix the implementation before treating a new known-bug test as part of the ordinary suite, then retain it permanently as an active regression test.

Run routine checks from the workspace root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
