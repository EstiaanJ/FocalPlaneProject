---
aliases:
  - Bugs
  - Known bugs
  - Regression tests
---

Each audit's bugs live under their own level-one dated heading. Future bug reports should be appended as new `# Bug report — YYYY-MM-DD` sections rather than replacing or mixing with earlier findings.

# Bug report — 2026-08-19

This report records confirmed defects found during the project-wide audit on 2026-08-19. None of these bugs has been fixed yet. [[Architecture Decisions]] records the human decisions made in response; this report remains the defect ledger until the implementations and regression tests are corrected.

Each bug has a focused regression test. The tests are ignored by the ordinary test run so that routine checks remain useful, but they can be run explicitly and currently fail:

```sh
cargo test -p focal-core -- --ignored
cargo test -p exposure-curve-tool -- --ignored
cargo test -p better-plots -- --ignored
```

Once a bug is fixed, remove its `#[ignore]` attribute and keep the test as a permanent regression test.

## Confirmed bugs

| ID | Area | Severity | Problem |
| --- | --- | --- | --- |
| FP-CORE-001 | FocalCore | High | An unsupported pipeline version is silently rendered as though it were the current version. |
| FP-CORE-002 | FocalCore | High | Non-finite module parameters can be accepted when a particular input happens to produce finite pixels. |
| FP-CURVE-001 | Exposure Curve Tool | High | Adobe RGB values are converted and clipped to the smaller sRGB gamut before the editable curve. |
| FP-PLOTS-001 | Better Plots | Medium | Forward scope analysis and reverse highlighting disagree about semi-transparent pixel colour; the decided opaque-input boundary will remove this unsupported internal state. |
| FP-PLOTS-002 | Better Plots | Medium | The RYB hue mapping uses piecewise-linear interpolation rather than the documented darktable vectorscope spline. |
| FP-PLOTS-003 | Better Plots | Medium | JPEG EXIF orientation is ignored when loading the image. |

### FP-CORE-001 — unsupported pipeline versions are accepted

`PipelineSnapshot` saves a version, but `Pipeline::render` never checks it. A snapshot with `version = PIPELINE_VERSION + 1` currently renders successfully. This could silently give an edit a different meaning from the one that was saved.

Rapid development does not require migrations, but it does require refusing state whose meaning is unknown.

Regression test: `pipeline::tests::unsupported_pipeline_version_is_rejected` in `crates/focal-core/src/pipeline.rs`.

### FP-CORE-002 — parameter validity depends on pixel values

FocalCore checks whether output pixels are finite after each module, but it does not validate parameters themselves. Negative-infinite exposure produces a finite gain of zero, so the render succeeds even though the parameter cannot be represented in ordinary JSON and is not a valid stable edit value. Other non-finite values may be caught only incidentally because of the pixels they happen to process.

Module parameters should be validated independently at snapshot construction or render entry.

Regression test: `pipeline::tests::non_finite_parameters_are_rejected_even_when_the_pixel_result_is_finite` in `crates/focal-core/src/pipeline.rs`.

### FP-CURVE-001 — wide-gamut information is clipped before the curve

The curve tool converts Adobe RGB to linear sRGB, clips the result to the sRGB gamut, encodes it, and only then applies the editable curve. Two different Adobe RGB reds can therefore both become encoded sRGB red `1.0` before the curve sees them.

This is irreversible data loss and conflicts with the decided canonical Adobe RGB (1998) perceptual curve domain. Adobe RGB to sRGB conversion and gamut handling belong after the editable curve.

Regression test: `pipeline::tests::wide_gamut_values_remain_distinct_until_after_the_editable_curve` in `crates/exposure-cruve-tool/src/pipeline.rs`.

### FP-PLOTS-001 — semi-transparent colour changes between forward and reverse lookup

Forward analysis accumulates premultiplied colour and divides by accumulated alpha, recovering the unassociated source colour. Reverse highlighting multiplies a pixel by alpha without dividing it back out. A half-transparent red is therefore plotted at full red chroma but searched for at roughly half chroma, so hovering the plotted red does not select the source pixel.

The alpha policy is now explicit: warn, obtain confirmation, and flatten to opaque RGB at the shared file boundary before analysis. Until that boundary is implemented, the two directions still disagree and the regression remains useful evidence of the current defect. Internal semi-transparent scope processing is not a product requirement.

Regression test: `vectorscope::tests::semi_transparent_pixels_use_the_same_colour_in_analysis_and_reverse_highlighting` in `crates/better-plots/src/vectorscope.rs`.

### FP-PLOTS-002 — RYB interpolation does not match the reference

The implementation connects the seven RYB hue knots with straight segments. The darktable vectorscope builds cubic-spline interpolation tables from those knots. The current mapping has a large first-derivative discontinuity at the first internal knot: the measured slopes are approximately `2.00` and `0.83` on either side.

This is visible as uneven angular expansion and contradicts [[Vectorscope Research]], which describes the spline as an important part of the reference appearance.

Regression test: `vectorscope::tests::ryb_hue_mapping_has_a_continuous_slope_at_internal_knots` in `crates/better-plots/src/vectorscope.rs`.

### FP-PLOTS-003 — JPEG orientation is ignored

Better Plots decodes a JPEG directly to RGBA without applying its EXIF display orientation. A two-pixel-wide JPEG tagged as orientation 6 remains `2 × 1` instead of becoming `1 × 2`. The displayed photograph, spatial selection, and scope analysis therefore use the wrong orientation.

The Exposure Curve Tool already handles the same case correctly and provides a useful local implementation reference.

Regression test: `loader::tests::jpeg_exif_orientation_is_applied_before_scope_analysis` in `crates/better-plots/src/loader.rs`.

## Risks which need decisions or more evidence

These are not recorded as confirmed bugs yet:

- ICC identification in the curve tool searches profile bytes for names such as `Adobe`, `A98`, and `sRGB`; it is not a real ICC parser and may misidentify custom profiles.
- Better Plots deliberately assumes decoded sRGB and ignores embedded profiles. The UI discloses this, but it limits the diagnostic meaning of the scopes.
- Background workers reject stale results, but active Better Plots analysis and reverse-highlight scans cannot be cancelled. Large images may delay a click-driven search even though stale results are eventually discarded.
- FocalCore has no cancellation or progress interface yet, despite those being central preview requirements.
- Current curve code and UI may still say “Luminance” for weighted encoded channels. The decided term is **Luma**, using coefficients appropriate to the canonical Adobe RGB primaries.

See [[Project Audits#Project audit — 2026-08-19|Project audit — 2026-08-19]] for the architectural and product implications.
