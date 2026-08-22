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

This guide is for building a useful set of paired Fujifilm X-T5 RAW and camera-produced Standard JPEG images. The JPEG records what the camera did to the same sensor exposure; it is the reference for FocalPlane's opinionated no-edit **Camera-Neutral** rendering. Camera-Neutral is FocalPlane terminology, not the name of a Fujifilm film simulation. Existing fixture filenames retain `PROVIA` so that recorded source identities do not change.

This is a **relative rendering comparison**, not a calibrated measurement of the paint or scene. A labelled blue region means “make this RAW region behave like the camera JPEG's rendering of this particular blue”, not “this is the true colour of the paint”. The highly chromatic acrylic swatches are useful precisely because they exercise difficult hues, but they should be combined with less saturated colours, neutrals, natural objects, and held-out examples.

RAW research has resumed with the annotated X-T5 fixture. Captures and annotations remain useful independently of decoder or rendering implementation work.

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
- Use the camera's Standard JPEG as the Camera-Neutral reference and record every JPEG setting which can affect appearance: dynamic range, highlight, shadow, colour, sharpness, high-ISO noise reduction, clarity, grain, Colour Chrome effects, lens optimisation, colour space, and camera firmware.
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
test-image/X-T5_RAW/camera-neutral/
├── captures/
│   ├── xt5_camera_neutral_swatches_000.RAF
│   ├── xt5_camera_neutral_swatches_000.JPG
│   ├── xt5_camera_neutral_swatches_m1.RAF
│   └── xt5_camera_neutral_swatches_m1.JPG
├── annotations/
│   └── xt5_camera_neutral_swatches_000.regions.json
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

Rectangular regions are more useful than individual coordinates because they allow robust statistics and reveal an uneven or contaminated sample. Coordinates should use the final camera-JPEG pixel grid, with `(0, 0)` at the top left. Record the image dimensions so the coordinate system is unambiguous. The existing `PROVIA_JPG.JPG` fixture is 7728 × 5152 pixels.

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
  "pair_id": "xt5-camera-neutral-swatches-000",
  "raw": "xt5_camera_neutral_swatches_000.RAF",
  "reference_jpeg": "xt5_camera_neutral_swatches_000.JPG",
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
- a short note identifying the desired Camera-Neutral baseline;
- any subjective observations such as “the camera keeps this orange brighter” or “the rendered blue should not drift towards cyan”.

The existing `PROVIA_JPG.RAF` and `PROVIA_JPG.JPG` are the first Camera-Neutral pair from the same shutter release: the RAF contains the sensor data from which the camera made that JPEG. They have the same capture timestamp and exposure and record X-T5 firmware 4.00, Standard dynamic range, neutral highlight and shadow tone, sRGB output, disabled Colour Chrome effects, and a fixed 5500 K white balance.

Every entry in the accompanying rectangle table, and every isolated crop, was derived from that one JPEG. They are multiple observations within one paired image, not independent captures. The table contains 38 measured regions with matching crops, and the crop dimensions agree with the recorded rectangle bounds. The manually supplied names identify the highly saturated paint swatches; the other regions add less-saturated swatches and colourful scene content. An unnamed region still has a valid numerical target and must not be assigned an invented colour name.

Initial decoder research reached the correct 7728 × 5152 active crop, but generic development was substantially too dark and green compared with the JPEG. That experiment was deliberately removed rather than retained as production code. The new regions make the pair substantially more useful: fit only on a declared, hue- and saturation-stratified training subset, reserve other regions for validation, and report both per-region and whole-image error. Highly saturated swatches must not dominate the objective merely because they are the named samples. A transform fitted to all 38 regions and assessed on those same regions is not evidence of generalisation, and held-out regions from the same photograph do not replace later validation on independent captures.

## Current automated comparison

The `focal-io` examples provide a repeatable research path:

```sh
cargo run --release -p focal-io --example develop_xt5_reference -- \
  test-image/X-T5_RAW/PROVIA_JPG.RAF /tmp/xt5-reference.png

cargo run --release -p focal-io --example crop_xt5_regions -- \
  test-image/X-T5_RAW/PROVIA_JPG.RAF \
  test-image/X-T5_RAW/grid/PROVIA_JPG-rectangles.csv \
  /tmp/xt5-regions

cargo run --release -p focal-io --example calibrate_xt5_camera_neutral -- \
  /tmp/xt5-reference.png \
  test-image/X-T5_RAW/grid/PROVIA_JPG-rectangles.csv
```

The crop command creates one 16-bit developed-RAW crop for each of the 38 rectangles on the JPEG's 7728 × 5152 coordinate grid. Rawler reports that grid as the RAF's recommended crop beginning at sensor coordinate `(12, 21)`.

The calibration example evaluates a quadratic encoded-RGB research fit with every third rectangle withheld. The first fit was invalidated after 1:1 inspection revealed that Rawler's default developer had treated the 6×6 X-Trans CFA as Bayer data, producing a repeating green grid. That fit and Camera-Neutral v1 were discarded.

Camera-Neutral v2 introduced explicit X-Trans-aware interpolation. On this corrected RGB source, fitting reduced regional RGB RMSE from about 40 to 6 code values on the training regions and from about 36 to 9 on the withheld regions. Whole-image and 1:1 checks no longer show the CFA grid.

Human comparison then identified rectangle 30's orange as visibly too strong. Measurement confirmed that v2 rendered its average near `[255, 137, 47]` instead of the camera JPEG's `[215, 140, 48]`. Camera-Neutral v3 adds a smooth residual fit over all 38 supplied regions, bringing rectangle 30 to approximately `[212, 141, 48]` and reducing all-region RGB RMSE from about 7.1 to 3.9 code values. V3 remains replaceable input rendering rather than a preset or editable adjustment. Its simple X-Trans interpolation and display-encoded fit are not the required permanent wide-gamut scene-referred architecture, and independent captures are still required to measure generalisation.
