---
aliases:
  - Project audit
  - Project Audit 2026-08-19
  - Plan to Address Project Audit 2026-08-19
  - Audit response 2026-08-19
  - Architecture audit
  - State of the project
---

Each audit lives under its own level-one dated heading. Append future audits as new `# Project audit — YYYY-MM-DD` sections, with the audit, human response, and outcome as subheadings.

# Project audit — 2026-08-19

## Audit

### Overall assessment

The project vision is clear: this is a personal-first rewrite centred on a fast, correct standalone editor, with no required catalogue, a real distinction between Save and Export, an opinionated pipeline, and rigorous human-directed engineering.

The main risk was no longer an unclear idea. It was that FocalCurve and FocalPlot could become alternative product architectures before FocalCore and Focal Editor established shared contracts. At audit time, the two experiments contained substantially more implemented code than FocalCore, neither used it, and Focal Editor was empty.

The experiments were valuable and reasonably well isolated, but the next milestone needed to consolidate what had been learned rather than keep widening them.

### What was working well

- The rewrite, product boundaries, and standalone-editor requirement were documented clearly.
- FocalCore already used explicit image semantics and an inspectable module order.
- Numerical work in the GUI prototypes was substantially separated from layout code and could mostly be tested directly.
- Preview work used immutable snapshots and rejected stale results.
- Unsafe Rust was forbidden, formatting and Clippy were clean, and the ordinary workspace suite had 48 passing tests.
- FocalPlot disclosed its decoded-sRGB limitation instead of presenting itself as a RAW diagnostic.
- The controlled 16-bit Adobe RGB fixture enabled fast experiments without making X-T5 file size part of every test.

### Architectural divergence

FocalCore was not a dependency of either GUI experiment. FocalCurve had its own image representation, decoding, metadata, colour conversion, render state, and worker; FocalPlot had another representation and worker. FocalCore itself lacked cancellation, progress, render-quality context, and cancellation checkpoints.

The audit recommended one production processing architecture, with the experiments remaining independently runnable but contributing validated GUI-independent work to FocalCore or a small shared boundary library.

FocalCurve also converted Adobe RGB into sRGB and clipped it before the editable curve, contrary to the intended order. This destroyed wide-gamut distinctions and became FP-CURVE-001.

The curve experiment had accumulated Smooth, Linear, Bezier, tension, and derivative editing. That breadth was useful for experimentation, but it risked exposing the colour-science-heavy complexity the ordinary editor is meant to avoid.

FocalPlot followed much of darktable's vectorscope design, but its RYB mapping used piecewise-linear interpolation rather than darktable's spline. Reverse highlighting also performed uncancellable full-image searches for changing hover queries.

### Subproject findings

#### FocalCore

FocalCore was compact, readable, GUI-independent, serialisable, and a good foundation for the permanent CPU reference. Its main gaps were:

- unsupported pipeline versions and some non-finite parameters were accepted;
- image and parameter contracts were too narrow and did not validate themselves;
- placeholder modules reported completion as if they had processed pixels;
- error context, cancellation, progress, and Preview/Export quality were absent;
- tests were mostly single-pixel tests, without controlled fixtures, serialisation round trips, order comparisons, or application contracts;
- crop and resize would have required a dimension-changing API which did not yet exist.

#### FocalCurve

FocalCurve had a strong controlled fixture, responsive immutable preview requests, a pure curve evaluator, useful invariants, and good tests around interpolation, metadata, orientation, histograms, and output tagging.

Its main gaps were premature sRGB clipping, heuristic ICC name matching, inaccurate “Luminance” terminology for encoded weighted channels, large and dense source modules, missing deterministic worker tests, and a prototype-only export path which must not migrate into Focal Editor.

#### FocalPlot

FocalPlot had useful scope mappings, forward and reverse selection, explicit stale-result checks, research documentation, and visible attribution.

Its main gaps were inconsistent alpha handling, non-reference RYB interpolation, ignored JPEG orientation, a misleading `sampled_pixels` label, egui types mixed into numerical scope analysis, ignored profiles, uncancellable analysis, eager computation, and an oversized application module.

#### Focal Editor and OLD_EDITOR

The empty Focal Editor directory was not a problem in itself: experimentation was intentionally happening first. `OLD_EDITOR` was correctly understood as two source files retained for GUI reference, not as an architectural or buildable foundation. A human-authored GUI description was still needed before editor implementation.

### Documentation and repository findings

- `exposure-cruve-tool` contained the historical `cruve` typo.
- `better-plots` lacked a settled canonical name.
- Folder documentation described the empty editor directory inaccurately.
- A test-image README referenced a crate and fixture which did not exist in this repository.
- Open questions duplicated decided pipeline-order guidance.
- Root project guidance, toolchain pinning, CI, and an architecture decision record were absent.
- Member-local and workspace build directories duplicated substantial build output.

### Reference-project comparison

darktable confirmed that its RYB vectorscope linearises sRGB-like input, builds cubic-spline RYB tables, averages pixel blocks before chromaticity conversion, separates logarithmic radial placement from density transfer, and constructs its trace from density and coloured rendering layers. FocalPlot's black background, adaptive sampling, density transfer, blur, and CIE tab were intentional differences; piecewise RYB interpolation was accidental.

darktable and RawTherapee also demonstrated how rapidly curve types multiply interaction states and tests. The audit therefore favoured one excellent ordinary curve while retaining technical variants as experiments.

Spektrafilm provided the useful testing pattern of comparing accelerated processing against a readable CPU implementation at stage boundaries, not only at final output.

The local vkdt copy was unavailable during the audit, but this did not change the decision to prefer an opinionated ordered pipeline over an MVP DAG.

### Recommended next work

1. Resolve the curve domain, alpha policy, production curve feature set, and initial sidecar semantics with the human owner.
2. Harden FocalCore with validation, execution context, broader tests, honest stage reporting, and explicit image replacement semantics.
3. Build the thinnest real editor slice: open one image directly, make one global edit, compare, save versioned state, export, and cancel obsolete previews.
4. Graduate only selected experimental algorithms into shared code while keeping both harnesses runnable.
5. Establish formatting, Clippy, tests, documentation checks, coverage diagnostics, deterministic concurrency tests, benchmarks, and repeatable human visual review.

The proposed success condition was a small editor that could use one real FocalCore adjustment responsively, reproduce saved state, and export deterministically without turning the experiments into alternate cores.

## Human response

For this project, “discuss” means asking the human owner a small number of questions, offering concrete options and consequences, and going back and forth until the decision is aligned.

The owner agreed that FocalPlane needs one processing architecture rather than three. Redundant work across FocalCore, FocalCurve, and FocalPlot should be identified and compared; the best implementation should be consolidated rather than preserving whichever version happened to be written first. The GUIs must remain independently runnable.

The owner directed the project to:

- add cancellation, progress, render quality, better error context, parameter validation, and substantially broader FocalCore tests;
- fix FP-CORE-001, FP-CORE-002, FP-CURVE-001, FP-PLOTS-002, and FP-PLOTS-003 before graduating the affected code;
- use test images and multi-pixel/full-slice tests rather than relying on one-pixel cases;
- keep the curve experiment broad enough to compare ideas, without assuming every experiment belongs in production;
- retain Linear, Bezier, and derivative source in FocalCurve, but initially integrate only Smooth with Linked RGB, Luma, and Per-channel RGB;
- defer influence-radius curve editing until after MVP;
- make reverse scope searching click-driven, let a new click replace the selection, and use right-click to cancel or clear it;
- treat semi-transparent image processing as outside the photographic product scope;
- separate numerical scope analysis from egui and make both experimental applications reusable widgets as well as standalone harnesses;
- rename the projects conceptually to FocalCurve/`focal-curve` and FocalPlot/`focal-plot`;
- leave Focal Editor empty until the owner provides a GUI description;
- retain OLD_EDITOR as reference only;
- remain in rapid development without compatibility migrations, while saving every edit parameter and rejecting state whose version is unknown;
- establish root guidance, shared build practice, CI-style quality gates, concurrency tests, benchmarks, and meaningful human review.

The owner agreed with the thin vertical-slice direction but later explicitly paused its implementation until the Focal Editor GUI description exists.

## Follow-up decisions and outcome

The discussion following the response settled the remaining architecture far enough to create [[Architecture Decisions]] and [[Clean Architecture Migration]]. In particular:

- FocalCore is the single production processing architecture.
- A project-owned `focal-io` boundary will handle shared decode, ICC, orientation, metadata, alpha-boundary, and encode responsibilities.
- The decoded-image MVP canonicalises inputs into the Adobe RGB (1998) perceptual curve domain and converts to output sRGB only after the editable curve.
- The proper RAW implementation must use a wide-gamut, scene-referred working domain; its exact primaries and encoding remain human-owned.
- Alpha is not processed internally. A genuinely transparent input triggers a warning and confirmation, then is flattened simply to opaque RGB if accepted.
- Saved adjustments use absolute parameter values.
- Unsupported schema or pipeline versions are rejected; migrations are deliberately absent during rapid development.
- Cooperative cancellation should normally be observed within 150 ms.
- Crop is deferred.
- Focal Editor code must wait for the human-authored GUI description.

Confirmed defects remain tracked in [[Bug Report]].
