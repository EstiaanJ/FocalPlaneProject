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

## Editor working set and batch workflow

1. When one photograph is opened, should every supported image in its containing directory automatically appear in the filmstrip? This is simple, but assumes that a filesystem directory represents a meaningful photographic session.

2. Should a filmstrip ever span more than one directory? If so, should the user explicitly create a temporary working set, and should that set survive application restarts?

3. Should the filmstrip have exactly one actively displayed photograph plus an independent multi-selection used as the destination for batch operations?

4. When the user navigates away from an edited photograph, should unsaved edits be persisted automatically, retained only for the current session with an unsaved marker, or require an explicit Save, Discard, or Cancel decision?

5. When edits are applied to multiple photographs, should they be applied immediately or through a confirmation or review step which names the copied categories? Are the right categories **preset/look**, **adjustments to the look**, **basic corrections**, **geometry**, and **everything**, and which controls belong to each?

## Other unresolved decisions

1. Presets are not a current priority. When preset work resumes, should an applied preset remain a live, revision-aware reference or be copied into the edit? If the preset changes later, should existing photographs retain their appearance, update automatically, or offer an explicit update?

2. How should the prototype JSON sidecar identify its source image: relative path, absolute path, filename plus content hash, or a combination? What should happen when the image or sidecar is moved?

3. Does push-to-update mean that controls change the pending parameters immediately but Focal Editor renders the preview only when the user presses an Update button?

4. Which specific wide-gamut scene-referred working space, encoding, and complete input/display/export path should replace the canonical Adobe RGB MVP domain for the proper RAW implementation? Adobe RGB is settled for the MVP; it is not the final scene-referred architecture.

5. Empirical Preview-versus-Export comparison is required for scale-dependent processing, as specified in [[Testing#Cancellation and responsiveness]]. What numerical and human-visual tolerances should decide when noise-reduction and local-contrast approximations differ too much from the display-sized Export result?

The module-order question has been resolved into the approved relative order in [[Architecture Decisions]] and the experimental method in [[Focal Core Pipeline]].
