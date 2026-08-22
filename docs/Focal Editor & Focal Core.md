---
aliases:
  - Focal Editor
  - Focal Core
  - Editor and Core
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Focal Editor & Focal Core

Focal Editor is a Rust photo-editing desktop application. It is currently the focus of the project. FocalLib and the separate preset editor are later projects.

Use egui and eframe for the GUI. Keep GUI code outside image-processing and curve-evaluation logic so those parts remain independently testable and reusable.

Shared decoding, orientation, ICC interpretation, metadata, transparency handling, and output encoding belong to the planned [[Architecture Decisions#Shared file and metadata boundary|`focal-io` boundary]]. FocalCore remains independent of file dialogs and file formats.

The GUI is a new implementation. Its current product scope and interaction principles are recorded in [[FocalPlane]], [[MVP]], and [[Sliders]]. The processing architecture remains centred on FocalCore; film-stock and film-specific processing concepts are not part of the new editor.

## Initial GUI slice

This vertical slice is implemented. The list below records its original scope rather than limiting subsequently approved work.

The first implementation was deliberately narrow:

- open one PNG or JPEG directly, without an import or catalogue step;
- show Before and After previews;
- expose exposure in stops and contrast from `-100` to `+100`;
- show the input/output histogram;
- save editable parameters as a versioned JSON sidecar;
- export an 8-bit sRGB PNG.

The current MVP scope excludes the advanced curve control and response-curve controls. Crop and FocalCore-backed scope analysis were subsequently approved and implemented in Phase One. These GUI scope decisions never permit a second processing pipeline.

The original MVP export is an 8-bit sRGB PNG. The current editor additionally offers 8-bit sRGB JPEG export with a quality slider; PNG remains lossless. Both formats receive the same full-resolution render, and the export worker uses the Optimized executor while retaining the CPU Reference as the parity oracle.

## Standalone editor

Focal Editor must be usable without FocalLib. Opening one photograph must never require importing it, creating a catalogue, or adopting the photo-management workflow.

Focal Editor should eventually support:

- careful editing of an individual photograph;
- rapid editing of many photographs from the same scene;
- copying a preset, later adjustments, selected corrections, or a complete edit to multiple selected photographs through interactive context menus.

## GUI and Focal Core boundary

There must be strict boundaries and strictly defined interfaces between the GUI and the image-rendering pipeline.

The image-processing pipeline is called Focal Core. It is shared among FocalPlane applications and should work through a CLI without the GUI, or through a well-defined API when called by the GUI.

FocalCurve and FocalPlot remain independently runnable experiments and visual test harnesses. Their validated GUI-independent processing must use or move into FocalCore rather than becoming alternate production pipelines. Their egui presentation and reusable widget code remain outside FocalCore.

Image preview processing takes place for the GUI but remains the responsibility of Focal Core.

Focal Core uses an explicit, modular, [[Focal Core Pipeline|opinionated processing order]]. Developers can rearrange modules for experiments, but Focal Editor users do not reorder them. A DAG remains a possible future evolution rather than an MVP architecture.

## Preview and responsiveness

The preview should not lie, but it must be fast and does not need to be full resolution. A visibly close approximation is acceptable. Finding the right compromise between preview accuracy and high-speed, real-time editing—especially with large X-T5 RAW files—will require experimentation.

The UI must remain responsive while processing. Preview rendering should follow a **latest request wins** model:

1. A control change requests a preview using the complete current edit state.
2. If another change occurs while that preview is processing, obsolete work should stop immediately where practical.
3. Processing restarts using both adjustments rather than finishing the stale request and queuing another.
4. An operation may be reused only in the rare case where independence is known and doing so cannot produce a stale preview.

Intermediate module results may be cached for previews when measurement justifies it. Changing a module invalidates its result and later work while potentially leaving earlier results available for reuse. Caching is optional for the first implementation. Each request operates on an immutable pipeline snapshot.

Anything under 500 ms would be nice for an ordinary preview update. This is an interaction target, not permission to block the UI for 500 ms.

Cooperative cancellation should normally stop obsolete work within 150 ms on the target system. Each render receives an immutable snapshot, cancellation token, progress reporter, and explicit Preview or Export quality. Stale progress and completion events must not replace the newest request.

The progress bar, cancellation behaviour, and push-to-update mode are important parts of the design. See [[Open Questions]] for the exact meaning of push-to-update, which is not yet settled.

## Internal photo representation

Inside the process, a photo should be represented as finite `f32` per channel per pixel. Some submodules may need different internal representations, and their contracts should make that explicit.

Floating point is not permission to erase semantics: each image must identify its domain, channel meaning, primaries and white point where applicable, and exposure convention. In the proper RAW architecture, the scene representation is unbounded by display range; clipping and quantisation belong at explicit display or output boundaries.

The binding decoded-image and future RAW colour contracts live in [[Architecture Decisions#Colour pipeline for the MVP]] and [[Architecture Decisions#Colour pipeline for the proper implementation]].

Fujifilm X-T5 support may be a first-class advantage even if this makes the software less generic. The no-edit X-T5 baseline is called **Camera-Neutral** and uses a camera-produced Standard JPEG as a relative rendering target. It is not presented as a Fujifilm film simulation or as calibrated scene colour. Other built-in JPEG appearances remain references rather than rendering modes unless the human owner decides otherwise.

## Target system

- Debian 13 with Cinnamon
- Ryzen 5 CPU
- 64 GB DDR4
- Nvidia RTX 3060 Ti with 8 GB VRAM and proprietary drivers

Linux desktop is the current target. Windows and macOS may be considered later; mobile and web are out of scope.

## English coding standard

Use British English where practical in code, comments, and documentation: `colour`, not `color`.

## Related documentation

Use the [[README|documentation home]] for the maintained index. The most direct specifications for this application are [[MVP]], [[Sliders]], [[Architecture Decisions]], and [[Focal Core Pipeline]].
