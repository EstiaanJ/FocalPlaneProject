---
aliases:
  - Clean architecture plan
  - Architecture migration
  - FocalPlane architecture instructions
---

# Clean architecture migration

This is the implementation guide for consolidating the current experiments into the architecture defined by [[Architecture Decisions]]. The initial Focal Editor GUI description is now documented in [[Focal-Editor Old GUI]]. Keep later GUI decisions in a dedicated description rather than silently expanding this migration guide.

The sequence below records the migration logic, not current completion status. The repository foundation, known-defect corrections, FocalCore execution and colour contracts, production scope extraction, and first editor slice are substantially implemented. The shared `focal-io` crate and canonical harness renames remain outstanding.

## Intended dependency direction

```text
Focal Editor ───────┬──→ focal-io ──→ FocalCore
                    ├──→ FocalCore
                    ├──→ FocalCurve reusable widget
                    └──→ FocalPlot reusable widget

FocalCurve harness ─┬──→ focal-io ──→ FocalCore
                    └──→ FocalCore

FocalPlot harness ──┬──→ focal-io ──→ FocalCore
                    └──→ FocalCore
```

FocalCore owns processing semantics, image contracts, module parameters, curve evaluation selected for production, scope analysis selected for production, ordered rendering, validation, cancellation, progress, and CPU-reference correctness.

`focal-io` owns decoding, orientation, ICC/profile interpretation, metadata boundaries, transparency detection/flattening policy inputs, and export encoding.

FocalCurve and FocalPlot own their egui widgets, interaction state, presentation, experimental-only behaviours, and independently runnable visual harnesses. Their reusable widgets may depend on FocalCore; FocalCore never depends on them.

## Intended names

The current directories retain historical names until the migration is intentionally performed:

| Current | Canonical Rust/folder name | Documentation/product name |
| --- | --- | --- |
| `crates/exposure-cruve-tool` | `focal-curve` | FocalCurve |
| `crates/better-plots` | `focal-plot` | FocalPlot |
| `crates/focal-editor` | `apps/focal-editor` may be considered later | Focal Editor |

Perform renames as their own mechanical step, update Cargo package names and every documentation link together, and run the complete workspace checks afterward. Do not mix renaming with processing changes.

## Migration rules

### Compare before consolidating

Do not assume FocalCore's current implementation is automatically the implementation to keep. For every duplicated responsibility:

1. identify every implementation and test;
2. write down their differing semantics;
3. compare behaviour with controlled inputs and relevant reference projects;
4. ask the human owner if the choice affects visible output or product behaviour;
5. preserve the best-understood behaviour behind one contract;
6. remove duplication only after callers use the consolidated path.

Likely duplicated responsibilities include transfer functions, image containers, RGB conversion, orientation, decoded-image preparation, render snapshots, progress, cancellation, stale-result rejection, and export encoding.

### Separate semantic processing from presentation

Pure analysis and processing code must not import egui types. In particular:

- FocalCore curve evaluation consumes numeric curve state and pixels, not widget control points tied to screen coordinates.
- Scope analysis returns numeric bins, coordinates, density, and source colours, not `egui::ColorImage` or `Color32`.
- FocalPlot converts numeric analysis into textures and draws it.
- FocalCurve converts pointer interaction into validated numeric curve state.

### Make invalid states explicit

- Validate pipeline versions before rendering.
- Validate every module's parameters independently of input pixels.
- Reject non-finite saved values.
- Validate image dimensions and buffer lengths at boundaries.
- Use errors which identify the failed boundary, module, parameter, and expected contract where practical.
- Do not report placeholder modules as successfully processed work.

### Keep the CPU reference readable

The CPU pipeline remains the definition of processing correctness. Optimised CPU and future GPU paths are checked at stage boundaries against it, following the useful parity-testing pattern in the local Spektrafilm reference.

Avoid fusing modules in the reference merely for speed. An accelerated implementation may fuse work only when tests show that processing semantics remain within an explicitly approved tolerance.

## Ordered implementation sequence

### Phase 0 — documentation and human GUI description

- Keep [[Architecture Decisions]] current.
- The initial description exists in [[Focal-Editor Old GUI]] and authorises the first narrow slice.
- Stop and ask the human owner before adding controls or workflows outside that description.

### Phase 1 — repository foundation

- Create the root project guidance and quality gates.
- Rename the experimental applications in an isolated change.
- Use the workspace root target directory and remove obsolete member-local build artefacts after confirming they contain no source material.
- Add automated formatting, Clippy, tests, and documentation-link checks.

### Phase 2 — fix known defects before extraction

Resolve the confirmed bugs in [[Bug Report]] before treating experimental code as production-ready:

- reject unsupported FocalCore pipeline versions without implementing migrations;
- validate all FocalCore parameters;
- remove premature sRGB clipping from the curve path;
- replace FocalPlot's RYB interpolation with the darktable-style spline;
- apply orientation through the shared I/O boundary;
- replace internal alpha ambiguity with the confirmed flatten-or-cancel boundary.

These defects are now corrected and their tests remain active regressions. New extraction work must preserve them.

### Phase 3 — build `focal-io`

- Define one decoded-image result containing pixels, dimensions, source profile information, source bit depth, metadata, orientation status, and transparency status.
- Apply orientation once.
- Use proper ICC interpretation and transforms; do not infer a profile by searching arbitrary bytes for a name.
- Require an explicit flattening policy after transparency is reported.
- Provide deterministic controlled tests for sRGB, Adobe RGB, missing/unknown profiles, orientation, opaque RGBA, transparent RGBA, malformed metadata, and unsupported formats.
- Keep file dialogs and confirmation UI in the calling application.

### Phase 4 — harden FocalCore execution

- Add parameter validation and exact-version rejection.
- Add the render execution context from [[Architecture Decisions#Render execution contract]].
- Test cancellation and stale-result rejection deterministically.
- Add multi-pixel controlled fixtures and full-slice tests using `test-image` where appropriate.
- Add serialisation round-trip, module-order comparison, boundary-error, and Preview-versus-Export contract tests.
- Keep this execution-hardening phase independent of geometry work; crop was approved and added later in MVP Phase One.

### Phase 5 — consolidate the MVP colour and curve pipeline

- Add explicit types/contracts for linear Adobe RGB, encoded Adobe RGB curve values, linear output sRGB, and encoded output sRGB.
- Implement and test the canonical MVP transform order from [[Architecture Decisions#Colour pipeline for the MVP]].
- Port only Smooth evaluation and Linked RGB, Luma, and Per-channel RGB modes into FocalCore.
- Use Adobe RGB-appropriate Luma coefficients.
- Keep Linear, Bezier, derivative, and other research interactions in the standalone FocalCurve source.
- Add tests proving that distinct Adobe RGB values do not collapse before the curve and that identical curve state is reproducible.

### Phase 6 — consolidate scopes and reusable widgets

- Move GUI-independent scope analysis behind a FocalCore API with no egui dependency.
- Keep density-texture construction and drawing in FocalPlot unless profiling demonstrates a reason to move a numeric part.
- Implement click-to-lock reverse selection and right-click cancel/clear.
- Rename `sampled_pixels` to reflect whether it counts sampled blocks or report a separately calculated source-pixel coverage.
- Preserve the standalone FocalPlot harness for human visual evaluation.

### Phase 7 — build the first Focal Editor vertical slice (complete)

The initial GUI description is documented. Implement only its currently approved slice:

1. open one decoded PNG or JPEG without import;
2. process it through `focal-io` and FocalCore;
3. expose the first approved global adjustment;
4. show responsive Before and After previews;
5. cancel superseded previews within the 150 ms budget;
6. save all edit parameters as absolute values in a versioned JSON sidecar;
7. export a colour-managed 8-bit sRGB PNG;
8. reproduce identical output under the controlled conditions in [[Testing]].

## Required tests and evidence

Every extracted processing component needs:

- identity and boundary tests;
- non-finite and invalid-state rejection;
- multi-pixel tests, not only one-pixel examples;
- a controlled source-to-output fixture where appropriate;
- serialisation round trips for saved parameters;
- cancellation and stale-result tests for long work;
- comparison against the readable CPU reference;
- manual human review for visible interaction or image-quality changes.

Timing-based concurrency tests should be avoided where a barrier, fake stage, or deterministic cancellation checkpoint can prove the state transition.

## Decisions which remain human-owned

Do not silently settle:

- the exact wide-gamut scene-referred working space and encoding after MVP;
- the permanent gamut-mapping algorithm;
- live preset references versus embedded preset snapshots;
- source identity and sidecar relocation behaviour;
- the Focal Editor GUI and approved OLD_EDITOR behaviours;
- acceptable preview approximations;
- which experimental curve interactions, if any, return after MVP.

When one of these blocks implementation, present the smallest meaningful set of options and consequences to the human owner.
