# FocalCurve

This sub-project is an experimental GUI for getting the curve interaction right before it is integrated into Focal Editor. Prioritise rapid visual iteration, clear image-processing semantics, and a polished control over adding unrelated editor features.

Keep this application independently runnable as a visual test harness while designing its curve editor as a reusable widget.

Use **curve** consistently in code, UI text, documentation, crate metadata, and filenames.

## Prototype layout

- Use egui and eframe for the GUI.
- Use a two-row layout.
- The top half shows Before and After previews side by side.
- Keep zoom and pan synchronised between both previews.
- The bottom half contains the curve editor.
- Keep the UI responsive while image processing runs.

## Initial image pipeline

Start with a controlled 16-bit PNG carrying an embedded Adobe RGB ICC profile. Do not add camera RAW support yet.

Keep these operations distinct:

1. Decode the image and interpret its embedded colour profile.
2. Convert values into the explicitly documented domain used by the curve.
3. Convert all supported inputs into the canonical Adobe RGB (1998) perceptual curve domain.
4. Apply the editable tone curve in that bounded domain rather than directly in linear light.
5. Decode the adjusted Adobe RGB values to linear light, then perform the output transform and gamut handling to sRGB outside the editable curve.
6. Encode and quantise the result as an 8-bit sRGB PNG for output.

Bit depth and dynamic range are not interchangeable. Do not describe the bounded 16-bit PNG input as equivalent to unbounded RAW scene data.

## Curve modes

The first prototype offers alternative modes for one curve stage rather than stacking several curve stages:

- **Linked RGB** applies one curve independently to red, green, and blue.
- **Luma** adjusts encoded brightness while attempting to preserve colour.
- **Per-channel RGB** provides separate red, green, and blue curves.

Keep mode-specific state if doing so is straightforward, but do not invent ordering rules for combining the modes. We are experimenting to learn which interaction is useful.

## Curve interaction

- The horizontal axis is input brightness and the vertical axis is output brightness in the curve's documented perceptual domain.
- The curve must remain a mathematical function: every input X has exactly one output Y.
- Control points must remain strictly ordered on X. Y does not have to be monotonic, so intentional inversions and unusual effects are allowed.
- Provide a smooth control-point curve mode.
- Linear, piecewise Bezier, derivative-source, and other experimental modes may remain available in this harness for comparison. Production integration is limited to Smooth curves with Linked RGB, Luma, and Per-channel RGB modes.
- Influence-radius interaction is deferred until after the MVP. Do not reintroduce it without a new decision.
- Prevent accidental spline overshoot unless the user explicitly creates that shape.
- Show the curve moving interactively while dragging, but render the After preview when the user releases the pointer rather than on every drag event.

## Histogram

Display a histogram behind the curve and provide a toggle between input and output histograms. The histogram must correspond to the values and channel mode represented by the current graph; label any approximation rather than presenting it as exact.

## Preview behaviour

- Preview accuracy may be visibly close rather than pixel-identical to final output.
- Superseded work should observe cancellation within 150 ms under supported conditions, and the UI must never block for that duration.
- If a new render request supersedes an active one, cancel or abandon the obsolete result and process the newest complete state.
- Progress and cancellation should be visible when processing is long enough to notice.

## Scope and architecture

- This is a prototype, not the complete MVP.
- Keep curve evaluation independent of GUI code so its production semantics can be consolidated into FocalCore. Do not maintain a competing production renderer here.
- Use the planned shared `focal-io` boundary for decoding, profiles, metadata, alpha handling, and encoding when that crate exists.
- Use an explicit, modular, ordered pipeline. Do not introduce a DAG or node-editor architecture for this prototype.
- Make experimental choices easy to switch or compare, especially curve interpolation, Luma coefficients, and histogram calculation. Influence-radius behaviour remains deferred.
- Add small controlled fixtures and numerical tests alongside visual testing.
- Follow the project-wide British English convention where practical.
- Keep the code modular, readable, maintainable, and tightly scoped. Aim for high coverage with meaningful tests rather than superficial coverage.
- Do not make significant interaction, image-processing, colour-science, or architectural decisions silently. Ask the human owner when the documentation does not settle a consequential choice, and use human visual feedback as one testing vector.

## Product character

The curve is intended to balance control and complexity. Sliders can hide too much of the underlying transformation; this control should make the input-to-output mapping intuitive without requiring the user to understand colour science. Prefer a focused, usable tool over a collection of technically possible options.
