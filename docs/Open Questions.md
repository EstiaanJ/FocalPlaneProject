---
aliases:
  - Unanswered questions
  - Design questions
  - Decisions needed
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Open Questions

These questions are deliberately unanswered. They should remain visible until experimentation or implementation gives us a good reason to decide. Resolved questions move to [[Architecture Decisions]] rather than lingering here as false uncertainty.

1. Presets are not a current priority. When preset work resumes, should an applied preset remain a live, revision-aware reference or be copied into the edit? If the preset changes later, should existing photographs retain their appearance, update automatically, or offer an explicit update?

2. When copying edits between photographs, are the right categories **preset/look**, **adjustments to the look**, **basic corrections**, **geometry**, and **everything**? Which controls belong to each category?

3. How should the prototype JSON sidecar identify its source image: relative path, absolute path, filename plus content hash, or a combination? What should happen when the image or sidecar is moved?

4. Does push-to-update mean that controls change the pending parameters immediately but Focal Editor renders the preview only when the user presses an Update button?

5. Which specific wide-gamut scene-referred working space, encoding, and complete input/display/export path should replace the canonical Adobe RGB MVP domain for the proper RAW implementation? Adobe RGB is settled for the MVP; it is not the final scene-referred architecture.

6. Empirical Preview-versus-Export comparison is required for scale-dependent processing, as specified in [[Testing#Cancellation and responsiveness]]. What numerical and human-visual tolerances should decide when noise-reduction and local-contrast approximations differ too much from the display-sized Export result?

The module-order question has been resolved into the approved relative order in [[Architecture Decisions]] and the experimental method in [[Focal Core Pipeline]].
