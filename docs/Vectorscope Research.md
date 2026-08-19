---
aliases:
  - Vectorscope
  - Darktable vectorscope research
  - Colour scope
---

# Vectorscope Research

## Goal

Create a vectorscope inspired by darktable's beautiful, softly coloured RYB display, but use a much deeper black background in FocalPlane.

The supplied screenshot appears to show darktable's **RYB vectorscope**. This is an inference from the perfectly circular hue ring and the painterly red-yellow-blue ordering. Darktable's CIELUV and JzAzBz modes instead derive their boundaries from colour-space chromaticities and generally do not form this regular circle.

This note describes behaviour and algorithms for an independent implementation. The principal local reference is [`src/libs/scopes/vectorscope.c`](/home/estiaan/code/Reference_Projects/darktable-master/src/libs/scopes/vectorscope.c), The [darktable scopes manual](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/shared/scopes/) provides the user-facing description.

## What the scope represents

A vectorscope discards spatial position and, in these modes, lightness. Each sampled pixel contributes a point where:

- angle represents hue;
- distance from the centre represents chroma;
- accumulated brightness represents how many sampled pixels fall into that region.

Neutral pixels collect near the centre. Strongly chromatic pixels lie farther out. A dense coloured region in the photograph becomes a brighter cloud of that hue.

Darktable offers three coordinate systems:

- **CIELUV `u*v*`**: established and comparatively inexpensive;
- **JzAzBz `AzBz`**: more computationally expensive and intended to be more perceptually uniform;
- **RYB**: a circular, artist-oriented hue arrangement suitable for colour-harmony overlays.

It also offers linear and logarithmic radial chroma scales. The logarithmic view expands low-chroma detail near the centre.

## FocalPlane's CIE 1931 tab

The first FocalPlane tab is a CIE 1931 *xy* chromaticity plot. This is deliberately a simpler, more familiar horseshoe than darktable's current CIELUV and JzAzBz modes; darktable remains the layout and interaction reference, not a claim that the spaces are interchangeable.

For decoded sRGB pixels, the prototype:

1. converts sRGB channels to linear light;
2. applies the standard sRGB/D65 RGB-to-XYZ matrix;
3. computes `x = X / (X + Y + Z)` and `y = Y / (X + Y + Z)`;
4. plots `x` over `[0, 0.8]` and `y` over `[0, 0.9]`, with the vertical axis increasing upward;
5. stores the average source display colour per bin so the image trace remains fully coloured rather than becoming a monochrome occupancy mask.

The background uses the CIE 1931 2° spectral locus sampled at 5 nm as a coloured outline over a black field and grid. The earlier prototype used softly tinted wedges from the D65 white point to the locus; those fills are intentionally removed so the image trace is not competing with a coloured background. The line between the 380 nm and 780 nm endpoints is the line of purples. This is a visual guide and not a gamut boundary for every possible input profile.

Darktable's linear/logarithmic vectorscope control changes radial placement, not merely alpha. FocalPlane follows that behaviour for RYB: the logarithmic option remaps radius with `log(1 + 29r) / log(30)` while preserving hue angle. CIE 1931 stays linear because it is a chromaticity diagram rather than a radial chroma scope. Dot sharpness is separate and changes the density-to-alpha response, so it can be tuned without changing plotted coordinates.

Hover traces are rendered as a separate inverse-colour layer. The image marker uses the inverse of the sampled decoded pixel; CIE trace bins use their stored average source colour, while RYB trace bins use the visible hue colour at that plotted angle. This keeps highlights readable against both dark and saturated trace colours.

## Why darktable's trace looks colourful

Darktable does not assign one flat colour to every histogram bin. It builds two separate raster layers:

1. A full-colour background mesh maps every angle to a hue.
2. A monochrome density image records how many pixels land in every vectorscope bin.

During drawing, the density image acts as an alpha mask over the colour mesh. A second white layer is applied through the same mask using a hard-light blend at approximately `0.55` alpha. This combination preserves the hue while making dense areas bloom toward bright pastel colours. Sparse areas remain dim and translucent.

The outer hue ring uses the same colour mesh at about `0.4` opacity, so the guide and trace share exactly the same hue orientation.

## Darktable's RYB mapping

Darktable's RYB model is inspired by Gossett's *Paint Inspired Color Mixing and Compositing for Visualization*. Because the original model is not reversible, darktable retains its cube hues but maps RGB hue to RYB hue and back using cubic spline tables.

For every sampled pixel, darktable's RYB mode conceptually does this:

1. Convert the incoming sRGB-like values to linear sRGB.
2. Convert RGB hue to the interpolated RYB hue arrangement.
3. Convert RYB RGB values to HCV: hue, chroma, and value.
4. Discard value for plotting.
5. Convert polar hue and chroma to Cartesian scope coordinates:

```text
angle = 2π × hue
x = cos(angle) × chroma
y = sin(angle) × chroma
```

Darktable multiplies both coordinates by `0.01`, which is merely an internal plotting scale and need not be reproduced if FocalPlane normalises its coordinates directly.

The important visual property is not that constant. It is the nonlinear, reversible hue remapping which gives yellow, red, blue, green, cyan, and magenta their painterly angular positions while leaving chroma as radius.

## Hue-ring construction

Darktable traces the six bounded RGB-cube edges which do not touch black or white:

```text
red → yellow → green → cyan → blue → magenta → red
```

It samples 48 intermediate hues between each neighbouring pair, for 288 samples around the ring.

In RYB mode, these samples receive evenly spaced angles. Their visible colours are converted back from RYB to RGB, then normalised so the largest RGB component is `1`. Adjacent samples become radial patches in a Cairo mesh extending from the neutral centre to the outer edge. Rasterising that mesh once produces a reusable colour texture.

For a FocalPlane implementation, a triangle fan or a CPU-generated RGBA texture can provide the same structure in egui. Generate the mesh at a useful fixed resolution and regenerate it only when its colour model, display profile, scale, or size changes.

## Pixel sampling and binning

Darktable uses the lower-resolution preview rather than the full developed image. Its manual explicitly warns that scopes can therefore differ from the final render.

The current implementation averages adaptive square pixel blocks (capped at one million sampled blocks) before converting each block to chromaticity. It then:

1. converts the sample to the selected chromaticity coordinates;
2. maps the linear normalised coordinate into a square `diameter × diameter` buffer;
3. increments the corresponding integer bin;
4. ignores samples outside the plotted range rather than clamping them onto the edge.

The analysis keeps those bins in linear scope coordinates. RYB's optional logarithmic radial transform is applied when the texture is rendered, using bilinear sampling so the expanded centre does not become a blocky nearest-neighbour image. CIE 1931 is always rendered in its linear xy coordinates.

Averaging before chromaticity conversion is not identical to converting four pixels and averaging their scope coordinates. Darktable's source explicitly identifies this as an unresolved trade-off. FocalPlane should choose and test the behaviour rather than inherit it accidentally.

The current prototype deliberately uses one deterministic CPU bin array per analysis request. If analysis becomes parallel later, per-worker arrays followed by a reduction should remain clearer and cheaper than an atomic increment for every sample.

## Density to visible intensity

Raw bin counts have a very wide range. Darktable first normalises density for source and scope size:

```text
normalised_density =
    (1 / 30) × (scope_width × scope_height)
             / (sample_width × sample_height)
             × bin_count
```

The value is clamped to `[0, 1]` and passed through an HLG/Rec.2020 output transfer-function lookup table to turn linear density into display-like intensity. The result is stored as an 8-bit alpha mask.

This density transfer is a major part of the look. A simple linear alpha produces either invisible sparse colours or immediately saturated dense colours. FocalPlane does not need to use HLG specifically, but it should use a documented, adjustable nonlinear density curve. Good candidates to compare are:

```text
linear:      I = clamp(k × count, 0, 1)
logarithmic: I = log(1 + k × count) / log(1 + k × peak)
exponential: I = 1 - exp(-k × count)
```

The current prototype uses the exponential form with an area compensation and a denominator of `12`, then applies the independent dot-sharpness exponent during texture generation. That intentionally gives sparse colours more presence on the near-black background than the earlier denominator of `18`; it remains a visual tuning parameter rather than a colour-science constant.

Test density normalisation across image resolutions. The same image downsampled to a different size should produce a visually similar trace.

## Optional logarithmic chroma scale

Darktable expands radial distances with base `30`:

```text
r_log = log(1 + 29 × r / r_max) / log(30) × r_max
```

Then it preserves the angle by scaling both Cartesian coordinates by `r_log / r`. Zero chroma remains at the centre.

This is separate from the density/intensity curve: radial logarithmic scale changes **where** samples appear, while density scaling changes **how brightly** occupied bins are drawn.

## Recommended FocalPlane rendering layers

Draw back to front:

1. Deep black background.
2. Very subtle concentric grid circles.
3. Coloured hue ring and six primary/secondary markers.
4. Colour-mesh trace masked by density.
5. Soft white brightening pass masked by the same density.
6. Neutral centre marker.
7. Optional colour-picker samples and colour-harmony guides later.

The prototype also supports the reverse diagnostic interaction. A pointer position in either scope is converted back through the same radial mapping used for drawing. The worker then converts every decoded image pixel into scope coordinates and creates a transparent overlay for pixels inside the pointer's adjustable radius. The overlay uses each source pixel's inverse sRGB colour, so the selected colour family remains visible regardless of the photograph's local hue. This is intentionally separate from the image rectangle tool: the rectangle is a spatial ROI, while scope hovering is a colour-space ROI.

Keep analysis and presentation separate:

- `VectorscopeAnalysis` owns sampling, colour conversion, binning, and density normalisation.
- `VectorscopeStyle` owns background, grid, opacity, trace brightening, and marker appearance.
- The egui widget owns layout and interaction, consuming already prepared textures or bins.

This separation makes the colour mathematics testable without screenshot testing the GUI.

## Deeper black FocalPlane appearance

Darktable draws a radial theme gradient from `graph_bg` at the scope to a slightly darker `graph_exterior`. Its default theme currently defines `graph_bg` as `#262626`; other themes can make it considerably lighter. The supplied screenshot appears to use a lighter grey theme.

For FocalPlane, begin with:

```text
scope centre:   #08090A
scope exterior: #030405
grid:           low-alpha neutral grey
```

These are experimental values, not a locked palette. Preserve a slight centre-to-edge gradient rather than using a featureless pure black fill; it keeps the circle readable without producing the large grey field visible in the screenshot.

A near-black background will make the coloured trace appear more saturated and brighter by contrast. Re-tune the hue-ring opacity, white hard-light contribution, and density gain against the new background instead of changing only the background colour. Check the result on the target display and through the application's colour-managed output path.

## Colour management

The scope must state which pipeline image and profile it analyses. A vectorscope calculated from working-space values, display-transformed values, and exported sRGB values can legitimately show different distributions.

Darktable's global colour picker samples after the completed pixel pipeline and works in the selected histogram profile. Its CIELUV and JzAzBz paths convert through profile-aware XYZ representations. The RYB path assumes sRGB-like values before linearising them, which is important context rather than a universally reusable rule.

For FocalPlane's first Adobe RGB input → sRGB output experiment, the most understandable default is to analyse the colour-managed **output preview** in sRGB. Later, an input/output toggle could help explain what the curve and gamut conversion changed, but the selected domain must always be labelled.

Generate the hue texture in a defined RGB space and pass it through the same display transform as the rest of the UI where practical. Otherwise the beautiful ring may be numerically colourful but inaccurate on a calibrated or wide-gamut display.

## Validation plan

### Numerical tests

- Neutral RGB values map to the centre.
- Increasing chroma at constant hue moves outward without changing angle.
- Hue wraps continuously at `0/1`.
- Empty input produces an empty trace.
- Out-of-range coordinates do not accumulate on the rim.
- Downsampled versions of the same image have comparable normalised density.
- Linear and logarithmic radial modes preserve angle.
- Binning is deterministic across supported thread counts.

### Controlled fixtures

- Neutral grey ramp.
- Full hue wheel with constant chroma.
- Hue/chroma plane.
- The seven-column `R, Y, G, C, B, M, R` gamut-ring fixture suggested in darktable's source, with black and white rows.
- One saturated colour patch repeated at different image sizes.
- Out-of-gamut and negative-value fixtures once the working pipeline supports them.

### Human visual checks

- Does the trace preserve the colourful, powder-like quality of the reference?
- Can sparse and dense colour populations both be read?
- Does the deeper background improve the image without making the ring garish?
- Is the neutral cluster visible without dominating the plot?
- Does resizing the widget preserve apparent density and line weight?

Human visual approval is required here alongside numerical tests, in accordance with [[Engineering Principles]].

## Open design choices

- Keep the prototype's CIE 1931 xy tab, or add darktable-compatible CIELUV/JzAzBz tabs later?
- Use darktable-like RYB interpolation, a simpler independently designed RYB mapping, or a perceptual hue space such as OKLCh for the first implementation?
- Analyse the input image, current working image, or colour-managed output preview by default?
- Use a fixed density transfer or expose a trace-intensity control?
- Blur the density mask slightly to obtain a softer powder trace, or preserve exact bins?
- Should the vectorscope be a passive diagnostic, or eventually support direct colour interaction and harmony guides?

These choices affect meaning as well as appearance and should not be decided silently by an implementation agent.

## Sources and further research

- [darktable scopes manual](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/shared/scopes/)
- [`vectorscope.c`](/home/estiaan/code/Reference_Projects/darktable-master/src/libs/scopes/vectorscope.c) — local darktable algorithm and Cairo rendering reference
- [`cie_colorimetric_tables.c`](/home/estiaan/code/Reference_Projects/darktable-master/src/external/cie_colorimetric_tables.c) — CIE 1931 2° standard observer values
- [`cie1931.h`](/home/estiaan/code/Reference_Projects/vkdt-master/src/tools/shared/cie1931.h) — an additional local reference for sampled CIE data
- [`color_ryb.h`](/home/estiaan/code/Reference_Projects/darktable-master/src/common/color_ryb.h) — local spline vertices for darktable's reversible RYB hue mapping
- Gossett, *Paint Inspired Color Mixing and Compositing for Visualization* — algorithmic inspiration cited by darktable
- Safdar et al., *Perceptually uniform color space for image signals including high dynamic range and wide gamut* — JzAzBz background
