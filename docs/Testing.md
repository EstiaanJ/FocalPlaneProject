---
aliases:
  - Quality
  - Test strategy
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Testing and quality

Use unit, integration, contract, end-to-end, and manual testing. Write tests first where useful, but full test-driven development is not a requirement.

Aim for high test coverage while prioritising meaningful assertions, edge cases, failure paths, and regressions over the percentage alone. Human visual and interaction evaluation is a required testing vector, especially for GUI behaviour and perceived image quality, but it complements rather than replaces automated testing. See [[Engineering Principles]].

The mostly single-threaded CPU Reference implementation should remain readable. The separate Optimized executor contains CPU multithreading and GPU acceleration and is always checked against Reference output.

The CPU reference defines correct [[Focal Core Pipeline|ordered pipeline]] results, not the only permitted scheduling or caching strategy. Preview caches and accelerated implementations must not change module order or processing semantics.

`cargo test -p focal-core --test optimized_pipeline` verifies the optimized CPU
path independently, including snapshots which the GPU cannot yet execute. GPU
execution is opt-in and must remain a separately measured implementation of
the same contracts. `cargo test -p focal-core --features gpu --test gpu_pipeline`
compares the GPU output and stage report with the CPU reference on the main
`test-image` fixtures, checks finite display-bounded output, and runs a
hardware-tolerant catastrophic-regression smoke gate. Use the release benchmark
example to record the Reference/optimized-CPU/GPU ratios; do not turn one
machine's core count or transfer overhead into a universal speed requirement.
On a validation machine with the fixtures and a usable adapter, set
`FOCAL_REQUIRE_GPU_TESTS=1` so an unavailable adapter or missing fixture is a
failure. Ordinary clean-clone testing may still skip hardware-dependent GPU
execution while compiling the GPU implementation; set `FOCAL_RUN_GPU_TESTS=1`
to opt into the adapter tests without making an unavailable adapter fatal.

The benchmark prepares owned CPU inputs before timing so image cloning does not
inflate Reference or Optimized CPU execution relative to borrowed GPU input.
The optimized CPU smoke gate can be tightened on a stable target with
`FOCAL_OPTIMIZED_CPU_MAX_SLOWDOWN`; it remains a catastrophic-regression check,
not a claim that one machine's timing is universal.

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

Interactive preview tests must prove that adjustment processing receives a display-bounded sample from the decoded source, while Export receives the untouched full-resolution source. The current editor retains the full source and resamples the requested visible region for each preview, subject to a one-megapixel cap; it does not cache a separate bounded source image. Keep the routing contract as a boundary regression test rather than testing only the sampling calculation. Small-image tests must prove that nearest-neighbour enlargement happens after original-size processing.

## Reproducible edit state

Once sidecar loading is implemented, loading the same starting image and the same saved edit state under tightly controlled conditions should export identical pixels and stable metadata, apart from deliberately volatile fields such as creation time.

This tests whether presets and saved edits capture enough information. It is not a promise of compatibility between development builds. During rapid development, processing changes may invalidate old saved edits and outputs.

The current editor writes versioned sidecars but does not load them. The loading boundary must reject unsupported schema or pipeline versions clearly when it is added; there are no migrations during this rapid-development phase.

Tests need to control or explicitly tolerate:

- random seeds for effects such as grain;
- GPU and parallel floating-point behaviour;
- encoder settings;
- metadata ordering and timestamps;
- application and pipeline version;
- colour transforms and output profiles.

Exact tolerances for accelerated previews and renders remain to be defined.

For scale-dependent modules, preview fidelity must be measured against the full-resolution result as it will actually be viewed. Build fixtures for decoded-image noise reduction, local contrast, and their combination. For each fixture and representative parameter setting:

1. process the full-resolution source at Export quality, then reduce it to a representative 1440p display-sized image;
2. prepare the source through the real preview downsampling path and process it at Preview quality using explicitly scale-adjusted parameters;
3. compare the two equal-sized results numerically and by human visual review;
4. repeat across source resolutions, preview scales, radii or strengths, and photographic content.

Local-contrast radii should initially scale with the source-to-preview dimension ratio. Noise reduction must be evaluated separately because source reduction changes the noise distribution before the filter runs; do not assume that radius scaling alone preserves its meaning. Include global colour and tone error, structural or multi-scale similarity, edge and halo behaviour, retained fine detail, and residual luminance and chroma noise. Any approximation must be deterministic and documented, and the experiment must establish both its fidelity and its speed benefit before it becomes Preview behaviour.

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

Use [[Accelerated Rendering Visual Checklist]] for the required human review of
GPU and parallel rendering. Record the adapter, build, fixtures, zoom levels,
and result; an unchecked checklist is not evidence that review occurred.

## Review records

Dated applications of this checklist live in [[Project Audits]]. Confirmed defects and their retained automated regressions live in [[Bug Report]].
