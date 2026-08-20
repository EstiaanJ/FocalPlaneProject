---
aliases:
  - Unanswered questions
  - Design questions
  - Decisions needed
---

# Open Questions

These questions are deliberately unanswered. They should remain visible until experimentation or implementation gives us a good reason to decide. Resolved questions move to [[Architecture Decisions]] rather than lingering here as false uncertainty.

1. When a preset is applied, should the edit retain a live reference to that preset or copy its current values into the edit? If the preset changes later, should existing photographs change?

2. When copying edits between photographs, are the right categories **preset/look**, **adjustments to the look**, **basic corrections**, **geometry**, and **everything**? Which controls belong to each category?

3. How should the prototype JSON sidecar identify its source image: relative path, absolute path, filename plus content hash, or a combination? What should happen when the image or sidecar is moved?

4. Does push-to-update mean that controls change the pending parameters immediately but Focal Editor renders the preview only when the user presses an Update button?

5. Which specific wide-gamut scene-referred working space, encoding, and complete input/display/export path should replace the canonical Adobe RGB MVP domain for the proper RAW implementation? Adobe RGB is settled for the MVP; it is not the final scene-referred architecture.

6. What preview approximations are acceptable, and how will we measure when a fast preview differs too much from the final render?





# Regarding the order of modules
To discover the best order, treat it as an empirical design problem:

  1. Define what each control means photographically.
  2. Implement modules so developers can reorder them easily.
  3. Render controlled test images through plausible orderings.
  4. Generate side-by-side contact sheets.
  5. Evaluate predictability, clipping, colour shifts, artefacts, and performance.
  6. Test the winning candidates on real photographs.
  7. Document why each stage occupies its position.

  Avoid testing every possible permutation. Most ordering decisions can be evaluated in pairs: exposure before/after contrast, saturation
  before/after tone mapping, sharpening before/after resizing, and so forth.

  A reasonable starting hypothesis is:

  Decode
  → Input colour conversion / linearisation
  → Orientation
  → White balance
  → Exposure
  → Highlight and shadow processing
  → Contrast and tonal curves
  → Creative colour controls
  → Noise reduction / sharpening as appropriate
  → Resize
  → Output colour transform
  → Quantisation, dithering, and encoding

  Crop is deliberately deferred for now. Its eventual position should be chosen from its defined semantics, not inherited accidentally from this hypothesis.

  For the PNG/JPEG prototype, the critical rule is to decode the sRGB transfer function before performing exposure-like operations. Applying
  exposure directly to gamma-encoded RGB will produce a different—and generally less physically meaningful—control.

  The “best” order is the one where controls behave consistently with their advertised meaning. If we cannot explain why moving a module
  changes the result, its semantics probably need defining more clearly before its position is locked.
