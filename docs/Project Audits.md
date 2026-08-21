---
aliases:
  - Project audit
  - Project Audit 2026-08-19
  - Plan to Address Project Audit 2026-08-19
  - Audit response 2026-08-19
  - Architecture audit
  - State of the project
---

# Project audit — 2026-08-19

This is the durable summary of the 2026-08-19 project-wide review. [[Architecture Decisions]] contains the resulting instructions, [[Clean Architecture Migration]] the consolidation guidance, and [[Bug Report]] the confirmed defects and regression tests.

## Assessment

The product direction was already clear: a personal-first, fast and correct standalone editor with no required catalogue, a real distinction between Save and Export, an opinionated pipeline, and rigorous human-directed engineering.

The main risk was architectural divergence. FocalCurve and FocalPlot contained substantially more working code than FocalCore and each had its own image representation, loading, colour handling, worker, and processing semantics. Focal Editor was not yet implemented. The experiments were valuable and reasonably isolated, but the next milestone needed to consolidate their validated work instead of allowing three production architectures to emerge.

The audit also confirmed that:

- FocalCore was a compact, readable and GUI-independent basis for the CPU reference, but needed stronger contracts, validation, cancellation, progress, quality modes, error context, and broader tests;
- FocalCurve had useful interaction research and controlled fixtures, but clipped Adobe RGB into sRGB before the editable curve and used prototype-only ICC and export handling;
- FocalPlot had valuable forward and reverse scope experiments, but its RYB interpolation, alpha handling, orientation, cancellation, and separation from egui needed correction;
- `OLD_EDITOR` was suitable only as a GUI reference, and a human-authored GUI brief was needed before building Focal Editor;
- shared quality gates, deterministic concurrency tests, controlled multi-pixel fixtures, and repeatable human visual review needed strengthening.

darktable supported the intended RYB spline and scope-rendering research. RawTherapee illustrated the cost of proliferating curve modes. Spektrafilm supported comparing accelerated processing with a readable CPU implementation at stage boundaries. These references informed the audit without becoming architectures to copy wholesale.

## Human response

The owner confirmed that FocalPlane needs one processing architecture centred on FocalCore. Duplicate implementations must be compared and the best-understood behaviour consolidated, while FocalCurve and FocalPlot remain independently runnable visual harnesses.

The owner also directed the project to:

- create a shared project-owned I/O boundary for decoding, ICC, orientation, metadata, transparency, and encoding;
- use canonical encoded Adobe RGB (1998) for the decoded-image MVP curve, converting to output sRGB only afterward;
- reserve a wide-gamut, scene-referred domain for the proper RAW implementation, with its exact space remaining a human decision;
- integrate only Smooth curves with Linked RGB, Luma, and Per-channel RGB into production initially, while retaining other curve experiments in FocalCurve;
- make reverse scope selection click-driven and cooperatively cancellable;
- keep alpha outside photographic processing, using a confirmed flatten-or-cancel file boundary;
- store absolute edit values, reject unknown versions, and avoid development migrations;
- keep obsolete processing within the 150 ms cancellation target;
- retain the old editor as a GUI reference without carrying over its film model or processing pipeline;
- use meaningful automated tests and human visual review together.

## Outcome

The audit produced the binding architecture and migration documents, the initial Focal Editor brief, and the regression ledger. The thin editor slice and later Phase One work have since been implemented; historical defects and their active tests remain in [[Bug Report]].

Future audits should be appended as new dated level-one sections containing only findings not already represented in canonical decision, status, or defect documents.
