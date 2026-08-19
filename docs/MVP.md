---
aliases:
  - Minimum viable product
  - MVP scope
---

# MVP

The MVP is defined primarily by what is deliberately excluded or delayed. Once the human owner declares the MVP done we can call it that and move on; the scope does not need to be frozen prematurely as an exhaustive feature checklist.

## First prototype

The first vertical slice is a prototype, not the complete MVP:

1. Open an already decoded PNG or JPEG.
2. Adjust exposure, contrast, and white balance.
3. Keep the UI responsive while updating the preview.
4. Compare the result with the original.
5. Save editable state in a JSON sidecar.
6. Export a rendered 8-bit sRGB PNG or JPEG.

Starting with PNG and JPEG makes controlled tests practical. X-T5 RAW files are very large and slow to iterate on, and it is difficult to make a controlled X-T5 source image. RAW support follows after the editor and processing architecture are established.

For colour-managed decoded inputs, the MVP uses the [[Architecture Decisions#Colour pipeline for the MVP|canonical Adobe RGB (1998) curve domain]]. Output conversion and gamut handling occur after the editable curve. The later camera-RAW implementation will replace this with a wide-gamut, scene-referred working domain.

For this prototype, exposure is measured in stops and contrast is a simple control from `-100` to `+100`.

## Explicitly delayed beyond MVP

- FocalLib and library management
- The separate, advanced preset-authoring application
- RAW formats from cameras other than the initially supported camera workflow
- Brushes, painting, retouching, masks, gradients, and other local adjustments
- Windows, macOS, mobile, and web support
- Long-term compatibility with edit files produced during rapid development
- Influence-radius curve editing
- Linear, Bezier, and derivative curve editing in production Focal Editor

The project is in a rapid-development phase. We can burn what came before rather than maintaining migrations or rendering old edits identically after the processing pipeline changes.

Development sidecars still carry an exact schema and pipeline version. Unsupported versions must be rejected clearly rather than interpreted using current semantics.

## CPU reference

Build a mostly single-threaded CPU reference implementation first, then use it to test optimisations such as GPU acceleration and CPU multithreading. It may remain permanently as the readable definition of correct processing.

“Mostly single-threaded” does not mean blocking the interface. The GUI and processing pipeline should communicate asynchronously. See [[Focal Editor & Focal Core#Preview and responsiveness]].

The reference implementation should define the [[Focal Core Pipeline|opinionated module order]] used by later accelerated implementations. Modules remain easy for developers to rearrange during experiments, but the MVP has no user-editable graph.

## Control principles

- Sliders should have numeric input boxes unless otherwise noted.
- Numeric controls should have a fine-adjustment mode while holding Control. In egui, use `drag_value_speed()` and add a tooltip for discoverability.
- Tooltips are important. Keep them concise and include relevant hotkeys.
- Add hotkeys for controls where practical.
- Holding `/` should overlay available hotkeys.
- Put a small square reset button on the left of every adjustable control.
- Right-clicking a control may eventually allow it to be locked; locking is not required for MVP.
- Never allow the scroll wheel to change a slider value.
- Scroll may zoom when the pointer is over a photograph view.
- Do not use anaemically thin scroll bars or tiny mouse targets.

## Related documentation

- [[Sliders]] — proposed controls and grouping
- [[Presets and Saved Edits]] — prototype sidecars and future formats
- [[Testing]] — correctness criteria
- [[Open Questions]] — remaining prototype decisions
