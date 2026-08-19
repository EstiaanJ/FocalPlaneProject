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

Start the first prototype with decoded PNG and JPEG fixtures. They are small, fast, and controllable compared with X-T5 RAW files. RAW decoding and large-file performance should be introduced after the editor and processing contracts work reliably.

MVP colour-pipeline fixtures should include 16-bit Adobe RGB PNGs with known pixel values and profiles, plus sRGB inputs which must be converted into the same canonical Adobe RGB curve domain. Include more than single-pixel tests: gradients, colour patches, small multi-pixel images, and whole image slices are needed to expose indexing, channel, stride, boundary, and state-leakage bugs.

Each processing stage needs numerical tests for its own contract and integration tests proving that the composed pipeline preserves the documented order and domains. The single-threaded CPU path is the reference; concurrency tests must prove that parallel scheduling, stale-result rejection, and caching do not change the accepted result.

## Cancellation and responsiveness

A superseded preview must stop doing useful work within **150 ms** under supported test conditions. Test cancellation between modules and within long-running modules, along with these invariants:

- an obsolete render can never replace a newer result;
- progress belongs to the correct immutable render request;
- cancelling preview work does not corrupt the last accepted result;
- export and preview use explicit render-quality modes without silently changing pipeline semantics.

The 150 ms target is a cancellation-latency contract, not permission to block the UI thread.

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

Human evaluation remains required for control behaviour, visual artefacts, perceived responsiveness, and any claimed preview/export equivalence. Record what a person is expected to inspect so “looks right” does not become an undocumented substitute for a test.
