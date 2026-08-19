## Controls to focus on
Just do Exposure control slider and contrast
Don't include the curve from focal-curves for now.

## Scopes and plots
Support the vectroscopes from focal-plots and port the old histogram

## Layout
Keep the old editor's high-level layout and visual language without blindly copying its code:

- Navigator at the top left, even though zoom controls are not implemented yet.
- Presets below the Navigator on the left.
- Histograms at the top right.
- Editing controls below the histograms on the right.
- The photograph viewer in the centre.
- A filmstrip along the bottom.
- The old editor's loading-bar styling in the top bar. The status remains
  there and the `FOCALPLANE` title in all caps sits at the far right of that
  same top bar.
- The major panels and sub-panels should be resizable rather than fixed: left/right rail widths, filmstrip height, Navigator height, and histogram-panel height.



## What's not needed
old Vectroscope
old response curve
crop controls

### Speed issues
When I open a large 40mp photo it takes a long time to load, which is okay, but it also takes a long time to update when changes are made. I want the image preview to be a fast preview that operates on a reduced image size (whatever can fit on screen). The original image should be cached in some way in case the user zooms in
