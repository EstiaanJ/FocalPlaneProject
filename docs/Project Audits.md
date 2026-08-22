---
aliases:
  - Project audit
  - Project Audit 2026-08-19
  - Plan to Address Project Audit 2026-08-19
  - Audit response 2026-08-19
  - Architecture audit
  - State of the project
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
  - status/archive
---

# Project audit — 2026-08-19

## Outcome

The audit produced the binding architecture and migration documents, the initial Focal Editor brief, and the regression ledger. The thin editor slice and later Phase One work have since been implemented; historical defects and their active tests remain in [[Bug Report]].

Future audits should be appended as new dated level-one sections containing only findings not already represented in canonical decision, status, or defect documents.

# Phase Two feature review — 2026-08-21

The implemented Phase Two features were reviewed against [[Testing#Feature review checklist]]. The unchecked RAW, HEIC, and additional-hotkey items in [[MVP]] remain explicitly unsupported and are not represented as partial file-format implementations.

| Feature | Contract, invalid state, and adversarial coverage | Async and routing evidence | Manual review still required |
| --- | --- | --- | --- |
| Clipping warnings | Masks are captured from the linear Adobe RGB to output-sRGB boundary before display clamping; Preview-only allocation is explicit. Black, white, red, blue, grey, and fallback display pixels are covered. | The mask travels with the immutable preview generation and is discarded with stale frames. | On a profiled 16-bit gradient, toggle highlights and lowlights independently at 1x and zoomed views; inspect saturated red/blue and neutral black/white patches for false warnings. |
| Loupe | Cursor geometry is clamped to the preview bounds and its UVs are relative to the actually displayed sampled texture. Boundary and sampled-region tests cover the coordinate contract. | It presents the accepted preview texture only; it does not initiate processing or mutate edits. | Toggle with `L`, move continuously across the image and letterboxing, and repeat at 1x and high zoom; verify the crosshair remains cursor-centred and detail follows the pointer. |
| Processing bar | Loading, processing, export-processing, and ready states have bounded fractions and distinct documented colours; panel layout reserves its height. | Loading, preview rendering, export rendering, and stale completion state are kept separate; obsolete exports are cancelled and ignored. | Resize the window and right rail, then open a large TIFF, change a control, and export; verify the bar never clips, overlaps controls, or reports Ready while work is active. |
| TIFF input | The decode boundary preserves dimensions, orientation, alpha detection, 16-bit precision, and embedded ICC-to-Adobe-RGB or unprofiled-sRGB contracts. | Decode and thumbnail work remain outside the GUI thread, carry cancellation tokens, and reject obsolete selected-image generations. | Open RGB/RGBA 8-bit and 16-bit TIFFs with orientation and Adobe RGB profiles; verify transparency confirmation, thumbnails, preview, and sRGB export. |
| Copy and paste edits | The copied value object contains all current absolute adjustments, including crop state, and target application revalidates crop safety for the destination dimensions. | Context-menu paste to another filmstrip item loads that item before applying the immutable copied snapshot; stale loads cannot install it. | Copy from an edited image, paste to the current and a different filmstrip image, then verify every control and crop result; try missing-copy and failed-load paths. |
| White-balance picker | The picker samples the immutable Before preview, rejects non-finite and unusably dark samples, and keeps warmth/tint within FocalCore's validated ranges. | Sampling is UI-only; applying a result creates a new immutable preview request and cannot reuse an adjusted output pixel. | Pick a neutral patch at image edges and at zoom, verify the dropper cursor/icon, confirm the picker exits after a successful sample, and cancel with `Escape`. |

The automated checks above are part of the ordinary workspace suite. The manual column is deliberately not marked complete by automated tests: visual artefacts, perceived responsiveness, and photographic usefulness still require a human run on the target desktop with the named fixtures.
