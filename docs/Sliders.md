---
aliases:
  - Slider controls
  - Editor controls
---

# Sliders

These controls follow [[MVP#Control principles|the shared interaction principles]]. Their processing belongs to [[Focal Editor & Focal Core#GUI and Focal Core boundary|Focal Core]], not the GUI.

## First prototype

### Exposure

Measured in stops.

### Contrast

A simple photographic control ranging from `-100` to `+100`.

### White balance

Initially represented by two controls:

- Warmth or temperature
- Tint

The correct behaviour for already rendered PNG/JPEG files versus future RAW files remains an [[Open Questions|open question]].

## Proposed later controls

### Luminosity Balance

Working title. Intelligently brightens the image, compressing highlights while preserving detail and bringing up shadows, or does the reverse for negative values.

### Highlights

Brings highlights up or down.

### Shadows

Brings shadows up or down.

### Brightness

Brings midtones up or down.

### Black Point

Adjusts the black point.

### Saturation

Adjusts saturation.

### Vibrance

Adjusts colour intensity while behaving differently from simple saturation. The exact model is not yet defined.

### Sharpness

Sharpening.

### Noise Reduction

Noise-reduction options, potentially including different strategies or algorithms.

### Local Contrast

Local contrast is a global photographic adjustment. It does not imply brushes or spatial masks.

### Exposure Curve

Develop the curve control separately so its interaction and appearance can be properly refined. It should support a global mode and per-colour-channel modes.

### Hue Mapping

The intended behaviour is not yet documented.

## Presets and copied values

Whether a control belongs in a reusable preset or a photo-specific edit depends on whether it could reasonably apply to any photograph. See [[Presets and Saved Edits]].
