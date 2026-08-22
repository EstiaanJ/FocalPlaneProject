---
aliases:
  - Slider controls
  - Editor controls
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Sliders

These controls follow [[MVP#Control principles|the shared interaction principles]]. Their processing belongs to [[Focal Editor & Focal Core#GUI and Focal Core boundary|Focal Core]], not the GUI.

## Implemented controls

### Exposure

Measured in stops.

### Contrast

A simple photographic control ranging from `-100` to `+100`.

### White balance

Represented by two controls:

- Warmth
- Tint

For decoded PNG/JPEG inputs these derive opponent-channel gains in linear Adobe RGB while preserving neutral lightness. Future RAW temperature and tint use separate camera-aware semantics.

### Local Contrast

A global spatial adjustment with Amount and Radius controls. It does not imply brushes or spatial masks.

### Saturation

Adjusts saturation with protection for highlights and already highly saturated colours.

### Noise Reduction

Decoded-image noise reduction with separate Luminance and Colour strengths. It is distinct from future camera-profiled RAW denoising.

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

### Vibrance

Adjusts colour intensity while behaving differently from simple saturation. The exact model is not yet defined.

### Sharpness

Sharpening.

### Exposure Curve

Develop the curve control separately so its interaction and appearance can be properly refined. It should support a global mode and per-colour-channel modes.

### Hue Mapping

The intended behaviour is not yet documented.

## Presets and copied values

Whether a control belongs in a reusable preset or a photo-specific edit depends on whether it could reasonably apply to any photograph. See [[Presets and Saved Edits]].
