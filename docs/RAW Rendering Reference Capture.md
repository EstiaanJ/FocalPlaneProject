---
aliases:
  - RAW test image capture
  - Camera JPEG reference capture
tags:
  - authorship/machine
  - audience/human
---

# RAW rendering reference capture

## Purpose

This guide is for building a useful set of paired Fujifilm X-T5 RAW and Provia/Standard JPEG images. The JPEG records what the camera did to the same sensor exposure; it is the reference for FocalPlane's eventual opinionated no-edit rendering.

This is a **relative rendering comparison**, not a calibrated measurement of the paint or scene. A labelled blue region means “make this RAW region behave like the camera JPEG's rendering of this particular blue”, not “this is the true colour of the paint”. The highly chromatic acrylic swatches are useful precisely because they exercise difficult hues, but they should be combined with less saturated colours, neutrals, natural objects, and held-out examples.

RAW processing is currently paused. These captures and annotations can be prepared independently and retained for when implementation resumes.

## Recommended minimum set

Start small enough that the set remains easy to inspect:

1. The existing swatch-and-objects scene, with approximately 12–20 clean regions labelled across saturated, moderately saturated, neutral, dark, and light areas.
2. The same scene at the chosen exposure, one stop under, and one stop over without moving the camera.
3. A neutral scene containing matte white, several greys, and black. A photographic grey card is helpful but not required for the relative comparison.
4. A natural scene containing skin, foliage, wood, fabric, or similarly familiar colours.
5. A scene with flat areas and fine detail for judging noise reduction and sharpening separately from colour.

Once this works under one stable light source, repeat a smaller subset under a distinctly different known illuminant. Do not begin with mixed lighting: it makes it hard to tell a colour-rendering error from a lighting difference.

## Camera and scene setup

- Use a tripod, fixed focus, fixed framing, and RAW+JPEG Fine from the same shutter release.
- Use Provia/Standard and record every JPEG setting which can affect appearance: dynamic range, highlight, shadow, colour, sharpness, high-ISO noise reduction, clarity, grain, Colour Chrome effects, lens optimisation, colour space, and camera firmware.
- Choose one canonical baseline and keep it fixed across that set. A clean starting point is DR100/Standard, fixed highlight/shadow/colour/sharpness settings, no optional effects, and a fixed white balance. If the desired default deliberately includes settings such as noise reduction +1, keep them and say so in the manifest.
- Use manual exposure and a fixed ISO where practical. Bracket by changing shutter speed, not aperture, so depth of field and lens rendering stay stable.
- Use a fixed Kelvin or custom white balance rather than Auto White Balance. Record the Kelvin value and red/blue shift. The value need not be scientifically correct; repeatability matters here.
- Prefer stable daylight or a high-quality, non-flickering continuous light. Avoid changing sunlight, mixed light, and cheap PWM-driven lamps.
- Keep glossy highlights off the sampled paint. Highly pigmented acrylic can be glossy and metameric, so use broad light, angle the card away from reflections, and keep the camera and light positions fixed. Do not use a polariser for only part of the set.
- Include a frame identifier in a slate or in the manifest, but do not cover the useful scene area.

Before dismantling the setup, view the JPEGs at 100% and check focus, clipping, reflections, camera movement, and accidental automatic-setting changes.

## Files and naming

Keep the camera's original paired files unchanged. A simple local layout is:

```text
test-image/X-T5_RAW/provia-baseline/
├── captures/
│   ├── xt5_provia_swatches_000.RAF
│   ├── xt5_provia_swatches_000.JPG
│   ├── xt5_provia_swatches_m1.RAF
│   └── xt5_provia_swatches_m1.JPG
├── annotations/
│   └── xt5_provia_swatches_000.regions.json
└── manifest.json
```

The large image files may remain ignored and local. The manifest and region annotations are small enough to review and version if they contain no private information. Each manifest entry should record:

- pair ID and exact RAW/JPEG filenames;
- camera, firmware, lens, focal length, aperture, shutter, and ISO;
- film simulation, JPEG colour space, white balance and shift, and all appearance settings;
- light source and any modifier;
- exposure-bracket position;
- whether the pair is intended for fitting or validation;
- anything visibly unusual, including glare, clipping, movement, or mixed light.

## Labelling sample regions

Rectangular regions are more useful than individual coordinates because they allow robust statistics and reveal an uneven or contaminated sample. Coordinates should use the final camera-JPEG pixel grid, with `(0, 0)` at the top left. Record the image dimensions so the coordinate system is unambiguous. The existing Provia JPEG is 7728 × 5152 pixels.

Describe what is visible rather than asserting a calibrated colour name: `high-chroma blue acrylic`, `muted blue-grey paint`, or `warm off-white ceramic` is better than an unmeasured Pantone or Lab value.

For each rectangle:

- place it well inside the swatch or object, away from borders, brush ridges, text, dust, glare, and shadows;
- make it large enough to contain many pixels, then allow the analysis to inset it further;
- note visible texture, a gradient, or a small reflection rather than silently accepting it;
- mark it as `fit`, `validation`, or `exclude`;
- reserve at least 25% of the good regions as validation regions and do not use them to tune the transform.

Example annotation:

```json
{
  "schema_version": 1,
  "pair_id": "xt5-provia-swatches-000",
  "raw": "xt5_provia_swatches_000.RAF",
  "reference_jpeg": "xt5_provia_swatches_000.JPG",
  "coordinate_space": {
    "origin": "top-left",
    "width": 7728,
    "height": 5152
  },
  "regions": [
    {
      "id": "blue-acrylic-01",
      "label": "high-chroma blue acrylic",
      "use": "fit",
      "rect": { "x": 1000, "y": 1000, "width": 160, "height": 160 },
      "notes": "Matte centre; avoids brighter brush ridge"
    }
  ]
}
```

The numbers above only demonstrate the format; they are not coordinates for the supplied image.

## How the comparisons should be processed

The RAW render and JPEG must first be registered to the same crop and orientation. Even though the existing RAW's active crop and JPEG are both 7728 × 5152, demosaicing or lens correction can move content slightly. Check alignment before comparing small regions, and avoid edges where a one-pixel shift changes the result.

For each inset rectangle, collect at least the median, a trimmed mean, spread, and clipping count. A median or 10% trimmed mean is less sensitive than a plain average to dust, texture, and specular pixels. Reject or subdivide regions with a large internal gradient. Retain both encoded sRGB summaries, which describe the visible JPEG, and linear-light values for modelling. Lab or perceptual differences are useful diagnostics, but do not turn the reference into calibrated scene colour.

Use the labelled regions to understand broad hue, saturation, neutral, and tone behaviour. Do not fit a flexible transform to every swatch in this one image and call the result complete. A low-order, constrained transform or a smooth LUT can be an initial hypothesis; it should preserve neutral behaviour and be judged on the held-out regions and additional photographs. A result which reduces training error while making the validation images worse is overfitting.

Colour and tone are only part of the camera JPEG. Sharpening, noise reduction, local contrast, lens correction, highlight handling, and demosaicing are spatial or exposure-dependent. Evaluate those with the bracketed, flat-area, and fine-detail scenes rather than trying to infer them from average swatch colours.

## What to provide when implementation resumes

The most useful hand-off is:

- unchanged RAW/JPEG pairs;
- one manifest with the complete camera and lighting settings;
- region files with descriptive labels and explicit fit/validation use;
- a short note identifying the desired canonical Provia baseline;
- any subjective observations such as “the camera keeps this orange brighter” or “the rendered blue should not drift towards cyan”.

The existing `PROVIA_JPG.RAF` and `PROVIA_JPG.JPG` are a useful first pair. Initial decoder research reached the correct 7728 × 5152 active crop, but generic development was substantially too dark and green compared with the JPEG. That experiment was deliberately removed rather than retained as production code. It shows that the pair can expose gross errors; the larger labelled set is what will distinguish a generally useful X-T5 rendering from a transform overfitted to one photograph.
