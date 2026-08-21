---
aliases:
  - Minimum viable product
  - MVP scope
---

# MVP

The MVP is defined primarily by what is deliberately excluded or delayed. Once the human owner declares the MVP done we can call it that and move on; the scope does not need to be frozen prematurely as an exhaustive feature checklist.

## First prototype

The first vertical slice is complete but remains a prototype rather than the complete MVP:

1. Open a decoded PNG or JPEG.
2. Adjust exposure, contrast, and white balance.
3. Keep the UI responsive while updating the preview.
4. Compare the result with the original.
5. Save editable state in a JSON sidecar.
6. Export a rendered 8-bit sRGB PNG.

Starting with PNG and JPEG makes controlled tests practical. X-T5 RAW files are very large and slow to iterate on, and it is difficult to make a controlled X-T5 source image. RAW support follows after the editor and processing architecture are established.

For colour-managed decoded inputs, the MVP uses the [[Architecture Decisions#Colour pipeline for the MVP|canonical Adobe RGB (1998) curve domain]]. Output conversion and gamut handling occur after the editable curve. The later camera-RAW implementation will replace this with a wide-gamut, scene-referred working domain.

For this prototype, exposure is measured in stops and contrast is a simple control from `-100` to `+100`.

## Next Phases & Features
The phase number is included like this, phase 1 is:  `1`
phase 2 is `2`

### Tabs
These are different editing tabs, the tab bar itself should go in the top bar in the center where there is space.
- Before and After (done) `1`
- Main photo view (full screen view of the photo with the current edits, rendered at display native res) `1`
- Plots, full screen view of the plots and scopes widget with a small photo preview `3`
- Transfer Curve `3`

### Clipping warning `2`
Toggle on and off, highlights clipped highlights and low lights (one toggle each)

### Zoom + Loupe
Loupe toggled by L key at mouse cursor (moves with cursor) `2`
Zoom and pan on the image preview (in any tab) `1`

### Crop
`1`
To crop, once they select the crop mode, the user should be able to draw a rectangle on the image preview in Main photo view tab. Drawing happens by clicking and dragging. Once they release:
- image remains uncropped but now with a rectangle drawn over it
- areas outside the rectangle are darkened slightly.
- The rectangle remains editable
- If the user presses enter or the crop tool button again the crop happens on the preview image, but as always there should be a cached original so the crop can be changed or removed (by clicking on the tool now a 3rd time)
The crop rectangle itself should have:
- handles on the sides for controlling the proportions
- a line extending perpendicular from the middle point of the top line with a handle at the end which allows the user to rotate the cropped region
The crop tool area consists of:
- The main crop button
- a reset button
- aspect ratio dropdown with some common aspect ratios
- two input boxes [x] linkButton [y] for controlling the aspect ratio manually
- the link button which changes whether the aspect ratio is locked


### Processing bar move
Move the processing bar to the bottom of the controls panel and make it about half it's current height (y). It shouldn't be affected by the scrolling up and down of the control panel and should never overlap with the controls, instead it should live in its own (non re-sizeable) area in the right side panel, above the filmstrip and below the controls area.
`2`

### Scope/plot controls + features
Colour picking `3`
Log/Linear toggle `1`

### Input Space and Format
Apple raw from iphone 12 pro max `2`
HEIC `2`
Jpeg `1`
Tiff `2`
sRGB (done)
AdobeRGB `1` (if not done)
PNG (done)
16 bit for any format that is implemented and supports it `1`

### Export with last
A way for the user to export to the same folder as the last exported image this session so they don't need to navigate etc. `1`

### Raw support
Apple DNG `2`
Fujifilm X-T5 `2`
Raw De-Noise `3`
Other pre-demosiac features `3`
Use an external tool / library for demosiac

### Other features
Copy and paste Edits `2`
Copy and paste Presets `3`
Queue export `3`
White balance picker `2`
Hotkeys `2`


### Features by Phase Checklist

**Phase one**

- [x] Tabs: Before and After
- [x] Tabs: Main photo view, rendered at the display's native resolution
- [x] Place the editing-tab bar in the centre of the top bar
- [x] Zoom and pan in the image preview in every tab
- [x] Crop tool:
  - [x] Draw, confirm, edit, reset, and remove a crop
  - [x] Darken the area outside the editable crop rectangle
  - [x] Resize handles and a rotation handle
  - [x] Common and manually entered aspect ratios
  - [x] Lock and unlock the aspect ratio
- [x] Scope/plot controls: Log/Linear toggle
- [x] Input: JPEG
- [x] Input: sRGB
- [x] Input: Adobe RGB through embedded ICC colour management
- [x] Input: PNG
- [x] Support 16-bit input for every implemented format which provides it
- [x] Export to the folder used for the previous export in the current session

**Phase two**

- [x] Clipping warnings, with separate highlight and lowlight toggles
- [x] Loupe at the mouse cursor, toggled with `L`
- [x] Move and reduce the height of the processing bar as described above
- [ ] Input: Apple RAW from iPhone 12 Pro Max
- [ ] Input: HEIC
- [x] Input: TIFF
- [ ] RAW support: Apple DNG
- [ ] RAW support: Fujifilm X-T5
- [x] Copy and paste edits within the current editor session
- [x] White-balance picker
- [ ] Hotkeys beyond the loupe and existing crop shortcuts

**Phase three**

- [ ] Tabs: full-screen plots and scopes with a small photo preview
- [ ] Tabs: Transfer Curve
- [ ] Scope/plot controls: colour picking
- [ ] RAW denoising
- [ ] Other pre-demosaic features
- [ ] Copy and paste presets
- [ ] Queued export

**Unassigned**

- [ ] Select and integrate an external demosaic tool or library

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
