---
aliases:
  - Processing pipeline
  - Pipeline architecture
  - Focal Core modules
  - Focal Core Graph
---

# Focal Core Pipeline

For the MVP, Focal Core uses an explicit, opinionated, ordered processing pipeline. Each stage is modular and can be rearranged or replaced without rewriting the stage itself, but ordinary users do not reorder stages and presets do not need to save a graph.

FocalCore is the single production processing architecture. File decoding and encoding remain boundaries owned by the planned `focal-io` library; experimental applications must not maintain competing production pipelines. See [[Architecture Decisions]] and [[Clean Architecture Migration]].

Module order matters—a nonlinear operation before exposure does not generally produce the same result as exposure before that operation—but exposing arbitrary order adds complexity without clear value to ordinary photo editing. A deliberate order gives us predictable controls, portable presets, simpler saved state, and a much clearer reference implementation.

## Modular stages

Each processing module should have:

- a clearly defined input and output image contract;
- serialisable parameters;
- an explicit version where saved-state compatibility requires it;
- no dependency on the GUI;
- deterministic behaviour under the controlled conditions in [[Testing]].

Image contracts must describe domain, channel meaning, colour space, dimensions, and exposure convention where applicable. `f32` alone is not a sufficient type.

The pipeline should be assembled explicitly from a list of modules rather than hidden across GUI callbacks or hard-coded as one indivisible rendering function. Developers must be able to reorder modules easily for experiments and tests even though users cannot do so in Focal Editor.

## Finding the opinionated order

There is no universally correct order independent of what each control is meant to do. We should discover the order experimentally:

1. Define the photographic meaning of each control before choosing its position. For example, decide whether exposure simulates changing scene exposure or merely brightens an already rendered image.
2. Build small controlled test images for gradients, highlights, shadows, neutral grey, saturated colours, and clipping.
3. Compare plausible orderings pairwise. Change only the order while holding parameters constant.
4. Test real photographs representing the actual workflow, including difficult X-T5 photographs once RAW support arrives.
5. Judge predictability as well as appearance. A good order should make a control behave consistently across different images and settings.
6. Record the chosen order and the reason for each boundary, especially scene-referred versus display-referred processing.
7. Lock the order for a pipeline version once presets and saved edits depend on it.

The CPU reference is the best place to run these experiments. Tests should make order changes cheap and differences visible rather than assuming the first order is correct.

## Preview execution

Each preview request should use an immutable snapshot of the complete ordered pipeline and its parameters. If another control moves, Focal Core can abandon the obsolete request and process the newest state without seeing a half-updated edit.

The render execution context contains cooperative cancellation, progress reporting, and explicit Preview or Export quality. Processing modules must check cancellation often enough that obsolete work normally stops within 150 ms on the target system.

Intermediate stage output may be cached for previews where measurement shows that it helps. Changing one stage invalidates that stage and later dependent work while potentially allowing earlier results to be reused. This is an optional execution optimisation, not part of the meaning of the pipeline and not a requirement for the first implementation.

Preview and export use the same module order and processing semantics. Preview may use reduced resolution and documented fast approximations; export uses full resolution and final-quality algorithms.

## MVP colour path

The decoded-image MVP uses one canonical encoded Adobe RGB (1998) domain for curve evaluation. Source profile interpretation and conversion into that domain happen before the curve; conversion and gamut mapping into output sRGB happen after it. Do not clip to sRGB before the curve.

This bounded perceptual curve domain is not the proper RAW working space. The camera-RAW implementation will use a wide-gamut, scene-referred domain under a later pipeline version.

## Possible future graph

The future technical preset editor may benefit from an ordered module stack, richer curves, and mathematical parameter mapping. That may provide most of the useful flexibility without becoming a visual programming language.

If real requirements later emerge for branching, multiple inputs, masks, reusable subgraphs, or parameter-control connections, the modular pipeline can evolve into a graph. Do not build graph IDs, arbitrary ports, graph scheduling, cycles, or graph serialisation before those requirements exist.

vkdt, listed with the [[Folder Structure#External folders|external reference projects]], is a useful reference for a mature two-level module and execution-node DAG, typed connectors, region-of-interest propagation, and topological scheduling. It demonstrates what becomes possible, not what Focal Core must implement for MVP.
