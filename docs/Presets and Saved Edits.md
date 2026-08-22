---
aliases:
  - Presets
  - Saved edits
  - Save and Export
  - Edit state
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Presets and Saved Edits

## Save is not Export

Focal Editor must distinguish clearly between **Save** and **Export**.

- **Save** is intended to preserve a native, editable document which can be opened and changed later.
- **Export** renders the image destructively into an output such as JPEG or PNG.

On first save, or when using Save As, the user should eventually be able to choose how to store the editable document:

- a native Focal Editor file, analogous to a PSD;
- an image plus a sidecar file;
- edit state stored in FocalLib when FocalLib is managing that photograph.

For the prototype and MVP, use a JSON sidecar. The current editor writes this state but does not yet load it. I generally dislike photo applications creating extra XML or other files throughout a photo library, so a sidecar must not become the only long-term workflow merely because it is the first implementation.

Original photographs should remain untouched except for explicitly appropriate metadata changes, such as an “edited by” software field. FocalLib may eventually store a UUID in metadata as a backup reference to its database.

## A preset is not saved edit state

Do not conflate a preset with the saved state of a particular edit.

A preset has a look which could apply to any photograph. Crop is specific to one photograph and does not belong in a preset. The useful test is:

> Could this apply to any photograph, or is it a fix or edit for this particular photograph alone?

A preset is a starting point. Applying it should produce technically consistent processing rather than trying to adapt itself until different photographs look perceptually identical.

The editor uses presets, not film stocks. The old film-stock concept and film references do not carry into this rewrite.

The future technical preset editor may expose an ordered module stack, richer curves, and mathematical parameter mapping. A full processing graph is only worth building if concrete requirements such as branching or multiple inputs emerge. Focal Editor should consume presets through focused photographic controls rather than expose the technical authoring interface.

## Edits on top of a preset

Copying settings between photographs is a central workflow, not a convenience feature.

RawTherapee can copy processing settings, but it does not provide an efficient distinction between a preset and changes made on top of that preset. For example, a preset may establish a particular contrast and I may reduce the contrast slightly for one scene. That later change is conceptually an adjustment to the preset, not necessarily a new look and not a crop-like photo-specific correction.

Saved state therefore needs to distinguish, at least conceptually:

1. the reusable preset or look;
2. adjustments made on top of that preset;
3. photo-specific edits such as crop and rotation.

The representation of those layers is still an [[Open Questions|open design question]]. Do not flatten away the distinction accidentally before it is resolved.

Adjustments are saved as **absolute parameter values**, not deltas from the preset. If a preset sets contrast to `+20` and I change it to `+15`, the saved adjustment is `+15`, not `-5`. The conceptual layering still matters for copying, UI, and future preset behaviour; it does not require relative arithmetic in the file format.

## Copying and batch work

Focal Editor should support interactive right-click menus for copying relevant settings and pasting them onto multiple selected photographs.

The user may want to copy a complete edit, but should also be able to copy categories such as the preset, adjustments made to it, basic corrections, or geometry without accidentally applying a crop from one photograph to an entire scene.

## Controlled reproducibility

With the exact same starting image, saved preset or edit state, application build, machine, and other controlled conditions, repeated export should produce identical pixels and stable metadata apart from intentionally volatile fields such as creation time.

This is primarily an integrity rule: if repeated output differs, the saved state may be missing information or its application may be buggy. Implementations must still account deliberately for random effects, parallel floating-point work, GPU behaviour, encoders, and metadata serialisation.

This promise does not require old edits to retain their appearance across development versions. See [[Testing]].

## Version handling during rapid development

Every sidecar must identify the exact edit-state schema and processing-pipeline version it expects. During this rewrite we do not maintain migrations or silently reinterpret old state. Unsupported versions must be rejected clearly instead of being loaded with subtly different processing.

Whether an edit keeps a live link to its source preset, and how a sidecar identifies a moved source photograph, remain deliberate [[Open Questions]]. See [[Architecture Decisions]] for the binding decisions already made.
