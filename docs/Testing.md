---
aliases:
  - Quality
  - Test strategy
---

# Testing and quality

Use unit, integration, contract, end-to-end, and manual testing. Write tests first where useful, but full test-driven development is not a requirement.

Aim for high test coverage while prioritising meaningful assertions, edge cases, failure paths, and regressions over the percentage alone. Human visual and interaction evaluation is a required testing vector, especially for GUI behaviour and perceived image quality, but it complements rather than replaces automated testing. See [[Engineering Principles]].

The mostly single-threaded CPU implementation should remain a readable reference against which GPU acceleration and CPU multithreading can be checked.

The CPU reference defines correct [[Focal Core Pipeline|ordered pipeline]] results, not the only permitted scheduling or caching strategy. Preview caches and accelerated implementations must not change module order or processing semantics.

## Controlled fixtures

Use decoded PNG and JPEG fixtures for small, fast, controlled tests. Add RAW and large-file fixtures only where their format or performance characteristics are the behaviour under test.

MVP colour-pipeline fixtures should include 16-bit Adobe RGB PNGs with known pixel values and profiles, plus sRGB inputs which must be converted into the same canonical Adobe RGB curve domain. Include more than single-pixel tests: gradients, colour patches, small multi-pixel images, and whole image slices are needed to expose indexing, channel, stride, boundary, and state-leakage bugs.

Each processing stage needs numerical tests for its own contract and integration tests proving that the composed pipeline preserves the documented order and domains. The single-threaded CPU path is the reference; concurrency tests must prove that parallel scheduling, stale-result rejection, and caching do not change the accepted result.

## Boundary and state-transition testing

Prioritise the boundaries implicated by the historical defects in [[Bug Report]]:

1. **Table-driven invalid-state matrices:** reuse cases for non-finite and out-of-range parameters, zero or mismatched dimensions, malformed metadata, unsupported versions, and invalid curve or crop state.
2. **Small decode-to-export fixtures:** verify orientation, dimensions, profile, alpha policy, bit depth, selected pixels, output tagging, and edit-state reproduction through the complete path.
3. **Deterministic concurrency models:** use controllable workers, barriers, or fake stages to force important completion orders, cancellation points, failures, and retries without relying on timing.
4. **Property-based tests:** target dimension/buffer relationships, crop geometry, curve ordering, colour round trips, finite-value preservation, and cancellation checkpoint bounds.
5. **Boundary-routing tests:** prove which source, image revision, edit snapshot, crop state, colour contract, and Preview/Export quality actually reach processing and encoding.

Adversarial cases should include empty, one-pixel, one-row, very wide, transparent, 8-bit, 16-bit, rotated, profiled, unprofiled, and malformed inputs, plus obsolete work completing after its replacement.

## Cancellation and responsiveness

A superseded preview must stop doing useful work within **150 ms** under supported test conditions. Test cancellation between modules and within long-running modules, along with these invariants:

- an obsolete render can never replace a newer result;
- progress belongs to the correct immutable render request;
- cancelling preview work does not corrupt the last accepted result;
- export and preview use explicit render-quality modes without silently changing pipeline semantics.

The 150 ms target is a cancellation-latency contract, not permission to block the UI thread.

Interactive preview tests must prove that large decoded sources are replaced by a cached, display-bounded render source before adjustment processing, while Export selects the untouched full-resolution source. Keep this as a boundary regression test: testing only the downsampling calculation is insufficient because the original performance regression was caused by routing the correct full-resolution buffer into the wrong render path. Zoom tests must likewise prove that only the visible source region is resampled to the physical preview dimensions, and small-image tests must prove that nearest-neighbour enlargement happens after original-size processing.

## Reproducible edit state

Under tightly controlled conditions, loading the same starting image and the same saved edit state should export identical pixels and stable metadata, apart from deliberately volatile fields such as creation time.

This tests whether presets and saved edits capture enough information. It is not a promise of compatibility between development builds. During rapid development, processing changes may invalidate old saved edits and outputs.

Sidecars with an unsupported schema or pipeline version must fail clearly. Tests should specifically reject them; there are no migrations during this rapid-development phase.

Tests need to control or explicitly tolerate:

- random seeds for effects such as grain;
- GPU and parallel floating-point behaviour;
- encoder settings;
- metadata ordering and timestamps;
- application and pipeline version;
- colour transforms and output profiles.

Exact tolerances for accelerated previews and renders remain to be defined.

Human evaluation remains required for control behaviour, visual artefacts, perceived responsiveness, and any claimed preview/export equivalence. Each visible feature should record a short repeatable checklist naming the fixtures, gestures, zoom levels, expected behaviour, and artefacts to inspect so “looks right” does not become an undocumented substitute for a test.

## Feature review checklist

Before merging processing, file-boundary, or editor work, verify:

- [ ] Input and output colour domain, encoding, range, alpha convention, dimensions, and version are explicit.
- [ ] Constructors and deserialisers reject invalid, non-finite, malformed, and invariant-breaking state.
- [ ] Relevant adversarial shapes, bit depths, profiles, transparency, and orientations are tested.
- [ ] Every asynchronous result has a complete identity, explicit cancellation owner, bounded checkpoints, and stale-result rejection.
- [ ] Jobs cannot overwrite unrelated state or caches, and failures are visible and retryable.
- [ ] Preview receives the intended bounded source region; export receives the intended full-resolution source and current edit snapshot.
- [ ] Unconfirmed geometry or stale UI state cannot reach processing or export.
- [ ] A boundary or integration test proves correct routing rather than testing only the helper calculation.
- [ ] Equivalent logic is not being duplicated across FocalCore, applications, or the planned `focal-io` boundary.
- [ ] The manual visual and interaction check is recorded and performed where behaviour is visible.

Correct cancellation is not enough by itself: measure the **150 ms** target on the target system with a repeatable benchmark or performance test.

## Phase Two feature review record

The implemented Phase Two features were reviewed against the checklist on
2026-08-21. The unchecked RAW, HEIC, and additional-hotkey items in `MVP.md`
remain explicitly unsupported and are not represented as partial file-format
implementations.

| Feature | Contract, invalid state, and adversarial coverage | Async and routing evidence | Manual review still required |
| --- | --- | --- | --- |
| Clipping warnings | Masks are captured from the linear Adobe RGB to output-sRGB boundary before display clamping; Preview-only allocation is explicit. Black, white, red, blue, grey, and fallback display pixels are covered. | The mask travels with the immutable preview generation and is discarded with stale frames. | On a profiled 16-bit gradient, toggle highlights and lowlights independently at 1x and zoomed views; inspect saturated red/blue and neutral black/white patches for false warnings. |
| Loupe | Cursor geometry is clamped to the preview bounds and its UVs are relative to the actually displayed sampled texture. Boundary and sampled-region tests cover the coordinate contract. | It presents the accepted preview texture only; it does not initiate processing or mutate edits. | Toggle with `L`, move continuously across the image and letterboxing, and repeat at 1x and high zoom; verify the crosshair remains cursor-centred and detail follows the pointer. |
| Processing bar | Loading, processing, export-processing, and ready states have bounded fractions and distinct documented colours; panel layout reserves its height. | Loading, preview rendering, export rendering, and stale completion state are kept separate; obsolete exports are cancelled and ignored. | Resize the window and right rail, then open a large TIFF, change a control, and export; verify the bar never clips, overlaps controls, or reports Ready while work is active. |
| TIFF input | The decode boundary preserves dimensions, orientation, alpha detection, 16-bit precision, and embedded ICC-to-Adobe-RGB or unprofiled-sRGB contracts. | Decode and thumbnail work remain outside the GUI thread, carry cancellation tokens, and reject obsolete selected-image generations. | Open RGB/RGBA 8-bit and 16-bit TIFFs with orientation and Adobe RGB profiles; verify transparency confirmation, thumbnails, preview, and sRGB export. |
| Copy and paste edits | The copied value object contains all current absolute adjustments, including crop state, and target application revalidates crop safety for the destination dimensions. | Context-menu paste to another filmstrip item loads that item before applying the immutable copied snapshot; stale loads cannot install it. | Copy from an edited image, paste to the current and a different filmstrip image, then verify every control and crop result; try missing-copy and failed-load paths. |
| White-balance picker | The picker samples the immutable Before preview, rejects non-finite and unusably dark samples, and keeps warmth/tint within FocalCore's validated ranges. | Sampling is UI-only; applying a result creates a new immutable preview request and cannot reuse an adjusted output pixel. | Pick a neutral patch at image edges and at zoom, verify the dropper cursor/icon, confirm the picker exits after a successful sample, and cancel with `Escape`. |

The automated checks above are part of the ordinary workspace suite. The manual
column is deliberately not marked complete by automated tests: visual artefacts,
perceived responsiveness, and photographic usefulness still require a human
run on the target desktop with the named fixtures.
