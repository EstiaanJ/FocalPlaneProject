---
aliases:
  - Vectorscope
  - Darktable vectorscope research
  - Colour scope
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Vectorscope Research

## Goal and reference

FocalPlane's vectorscope is inspired by darktable's softly coloured, powder-like RYB display, but uses a much deeper black background. This is an independent Rust implementation, not a structural translation of darktable.

The principal reference is [`vectorscope.c`](/home/estiaan/code/Reference_Projects/darktable-master/src/libs/scopes/vectorscope.c); the [darktable scopes manual](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/shared/scopes/) describes its user-facing behaviour. Preserve visible credit to darktable and to Gossett and Chen's *Paint Inspired Color Mixing and Compositing for Visualization*.

A vectorscope discards spatial position and lightness. Hue becomes angle, chroma becomes distance from the centre, and accumulated density becomes brightness. Neutral pixels collect near the centre; strongly chromatic populations form coloured clouds farther out.

## FocalPlane scope modes

### CIE 1931 xy

The default tab is a familiar CIE 1931 *xy* chromaticity plot rather than darktable's CIELUV or JzAzBz modes. For decoded sRGB pixels it:

1. decodes the sRGB transfer function;
2. converts linear sRGB to D65 XYZ;
3. calculates `x = X / (X + Y + Z)` and `y = Y / (X + Y + Z)`;
4. maps `x` over `[0, 0.8]` and `y` over `[0, 0.9]`;
5. retains average source display colour per bin so the trace remains coloured.

The background shows the CIE 1931 2° spectral-locus outline and line of purples over black and a restrained grid. It is a visual chromaticity guide, not a universal input-gamut boundary. CIE 1931 remains linear because it is not a radial chroma scope.

### RYB

Darktable's artist-oriented RYB mode retains hues from the Gossett model but uses spline tables to remap RGB hue to the RYB arrangement and back. FocalPlane follows the same essential behaviour:

```text
encoded sRGB-like sample
→ linear RGB
→ spline-remapped RYB hue
→ hue/chroma/value
→ discard value
→ polar hue and chroma coordinates
```

The plotted coordinates are:

```text
angle = 2π × hue
x = cos(angle) × chroma
y = sin(angle) × chroma
```

The important property is the nonlinear, reversible hue arrangement, not darktable's incidental internal scale. Forward plotting and reverse selection must use a genuine inverse pair between spline knots so a visible hue maps back to itself.

The hue ring follows the RGB-cube edges `red → yellow → green → cyan → blue → magenta → red`. Its guide colours and trace must share the same orientation.

RYB may use darktable's base-30 logarithmic radial transform:

```text
r_log = log(1 + 29r) / log(30)
```

This changes sample position while preserving angle. It is separate from the density curve, which changes visible intensity.

## Sampling, density, and rendering

The prototype averages deterministic adaptive square blocks, capped at one million sampled blocks, before converting each block into scope coordinates. It increments a linear bin array and rejects samples outside the plotted range rather than clamping them onto the edge. RYB's radial transform is applied during texture generation with bilinear sampling; CIE remains in linear xy coordinates.

Averaging pixels before coordinate conversion is not equivalent to averaging their resulting scope coordinates. That trade-off should remain explicit and tested. If analysis is parallelised later, per-worker bin arrays followed by reduction are preferable to an atomic update for every sample.

Raw counts need nonlinear display treatment. The current trace uses an exponential density response with area compensation, followed by a separate dot-sharpness exponent. Density should remain visually comparable when the same photograph or widget is resized.

The colourful appearance comes from separating colour and occupancy:

1. a hue texture supplies colour;
2. a density mask supplies opacity;
3. a restrained white brightening pass lets dense regions bloom toward pastel colours.

Draw the deep background and subtle grid first, then the hue ring, density-masked trace, brightening pass, neutral marker, and interaction overlays. Keep presentation parameters such as dot sharpness independent from colour-space analysis.

Suggested starting colours are `#08090A` at the centre and `#030405` at the exterior, with a low-alpha neutral grid. These are visual starting points, not a locked palette. A deeper background changes perceived saturation, so ring opacity and trace gain must be judged together on the target display.

## Interaction and separation of responsibilities

Image hover sampling and a spatial rectangle may analyse a source pixel or region. Reverse selection is deliberately click-driven: clicking a colour in either scope locks a colour-space region and starts one image scan; another click replaces it, and right-click cancels or clears it. Merely moving over the scope must not launch repeated full-image work.

Reverse highlights use inverse source colours so selected pixels remain visible over any hue. Spatial rectangles and reverse colour selection are different tools and must not share ambiguous state.

Keep numerical analysis separate from egui:

- FocalCore owns explicit colour-domain contracts, sampling, coordinates, bins, density data, cancellation, and reverse-selection results;
- FocalPlot owns texture generation, styling, layout, and interaction;
- loading, profiles, orientation, metadata, and transparency belong at `focal-io`.

The standalone harness may analyse decoded sRGB while that limitation is labelled. Production scopes should analyse an explicitly identified pipeline image. Working-space, display-transformed, and exported sRGB values can legitimately produce different distributions.

## Validation

Numerical tests should establish that:

- neutral RGB maps to the centre;
- chroma increases radius without changing hue;
- hue wraps continuously;
- empty and invalid input boundaries are handled explicitly;
- out-of-range coordinates do not collect on the rim;
- RGB↔RYB mapping round-trips between knots;
- linear and logarithmic modes preserve angle;
- density remains comparable across image and scope sizes;
- analysis is deterministic and stale or cancelled work cannot replace a newer result.

Useful controlled fixtures include a neutral ramp, hue wheel, hue/chroma plane, repeated saturated patches, and the `R, Y, G, C, B, M, R` gamut-ring fixture with black and white rows. Negative and out-of-gamut fixtures become relevant when the selected pipeline domain supports them.

Human review remains required. Check the powder-like trace, readability of sparse and dense populations, neutral visibility, line weight during resizing, reverse-selection behaviour, and whether the near-black presentation improves the scope without making its ring garish.

## Remaining choices

- whether CIELUV, JzAzBz, or another diagnostic view is useful alongside the existing CIE and RYB tabs;
- which explicitly labelled pipeline image production scopes analyse by default;
- whether trace intensity remains fixed or user-adjustable;
- whether a small density blur improves the powder appearance without obscuring the data;
- whether colour-harmony guides or direct colour editing belong in a later product phase.

Consequential changes to scope meaning, colour science, or interaction remain human-owned decisions.

## Sources

- [darktable scopes manual](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/shared/scopes/)
- [`vectorscope.c`](/home/estiaan/code/Reference_Projects/darktable-master/src/libs/scopes/vectorscope.c)
- [`color_ryb.h`](/home/estiaan/code/Reference_Projects/darktable-master/src/common/color_ryb.h)
- [`cie_colorimetric_tables.c`](/home/estiaan/code/Reference_Projects/darktable-master/src/external/cie_colorimetric_tables.c)
- [`cie1931.h`](/home/estiaan/code/Reference_Projects/vkdt-master/src/tools/shared/cie1931.h)
- Gossett and Chen, *Paint Inspired Color Mixing and Compositing for Visualization*
- Safdar et al., *Perceptually uniform color space for image signals including high dynamic range and wide gamut*
