---
aliases:
  - Bugs
  - Known bugs
  - Regression tests
---

Each audit's bugs live under their own level-one dated heading. Future bug reports should be appended as new `# Bug report — YYYY-MM-DD` sections rather than replacing or mixing with earlier findings.

# Bug report — 2026-08-19

This report records confirmed defects found during the project-wide audit on 2026-08-19. [[Architecture Decisions]] records the human decisions made in response, and this report remains the defect ledger with the regression tests which protect each correction.

The implementations and active regression tests now resolve FP-CORE-001, FP-CORE-002, FP-CURVE-001, FP-PLOTS-001, FP-PLOTS-002, and FP-PLOTS-003. FP-PLOTS-001 is made consistent in the current experimental harness; the planned `focal-io` transparency boundary remains the product-level replacement for internal semi-transparent processing.

Each bug has a focused regression test. The tests are now active in the ordinary suite. The entries below preserve the original defect descriptions and explain the correction or remaining product-level boundary.

```sh
cargo test -p focal-core
cargo test -p exposure-curve-tool
cargo test -p better-plots
```

The historical rule was to remove `#[ignore]` once a bug was fixed. The corrected tests remain permanent regression tests.

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

The alpha policy is now explicit: warn, obtain confirmation, and flatten to opaque RGB at the shared file boundary before analysis. The current harness keeps forward analysis and reverse highlighting consistent for its experimental semi-transparent inputs, while internal semi-transparent scope processing remains outside the product requirement.

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
- Background workers reject stale results, but active Better Plots spatial-analysis scans cannot be cancelled. Large images may still delay rectangle or hover analysis even though stale results are eventually discarded.
- FocalCore now exposes cancellation, progress, and Preview/Export quality; the 150 ms cancellation target still needs a target-system benchmark.
- Current curve code and UI may still say “Luminance” for weighted encoded channels. The decided term is **Luma**, using coefficients appropriate to the canonical Adobe RGB primaries.

See [[Project Audits#Project audit — 2026-08-19|Project audit — 2026-08-19]] for the architectural and product implications.

# Bug report — 2026-08-19 follow-up

The second non-editor bug-checking pass found and corrected the following
boundary defects. Their regression tests remain active.

| ID | Area | Problem | Regression test |
| --- | --- | --- | --- |
| FP-CORE-003 | FocalCore | Cancellation requested by a progress callback after the initial or final progress update was reported as a successful render. | `pipeline::tests::cancellation_during_initial_progress_cancels_an_empty_pipeline`, `pipeline::tests::cancellation_during_empty_completion_progress_does_not_report_success`, and `pipeline::tests::cancellation_during_final_progress_does_not_report_success` |
| FP-CORE-004 | FocalCore | Negative white-balance multipliers were accepted as valid parameters and could produce negative channel values. | `pipeline::tests::negative_white_balance_multipliers_are_rejected` |
| FP-PLOTS-004 | Better Plots | The duplicated harness scope mapping accepted out-of-domain linear source coordinates, and negative reverse-highlight radii were silently clamped. | `vectorscope::tests::linear_source_coordinates_reject_values_outside_display_domain` and `vectorscope::tests::reverse_highlight_rejects_negative_radius` |
| FP-PLOTS-005 | Better Plots | Reverse scope searches started continuously from pointer hover, could not receive clicks because the plot used hover-only sensing, and did not cancel active scans. | `app::tests::scope_hover_does_not_start_reverse_search`, `app::tests::scope_panels_capture_clicks_without_enabling_dragging`, and `loader::tests::cancelled_highlight_does_not_emit_a_stale_result` |

# Focal Editor follow-up — 2026-08-19

These defects were confirmed during the filmstrip performance review and are
fixed with active regression tests.

| ID | Severity | Confirmed defect | Regression test |
| --- | --- | --- | --- |
| FP-EDITOR-001 | High | Selecting a sibling rebuilt the filmstrip, discarded every cached thumbnail, and queued the directory again. | `selecting_a_sibling_does_not_rebuild_an_unchanged_filmstrip` |
| FP-EDITOR-002 | Medium | The thumbnail worker continued decoding obsolete queued directories even though their results could never be accepted. | `thumbnail_worker_drops_queued_requests_from_obsolete_directories` |
| FP-EDITOR-003 | High | A preview request could invalidate an unrelated in-flight image load because both used one generation identity. | `preview_requests_do_not_invalidate_an_in_flight_image_load` |
| FP-EDITOR-004 | High | Export could write an older accepted preview after the controls or selected source had changed. | `export_requires_pixels_from_the_current_completed_render` |
| FP-EDITOR-005 | High | Loading a main image replaced its small filmstrip texture with the full-resolution texture, retaining very large GPU allocations while browsing. | `loading_main_image_does_not_replace_cached_filmstrip_thumbnail` |
| FP-EDITOR-006 | Medium | Completed thumbnail work did not keep UI polling alive, so prefetched images could remain visually stuck until another interaction. | `pending_thumbnails_keep_ui_polling_after_other_work_finishes` |

Filmstrip loading is now demand-driven. It prefetches twice the number of
currently visible items, biases the spare viewport toward the available
scroll direction near either end, and requests a new window as the filmstrip
scrolls. `filmstrip_prefetches_twice_the_visible_thumbnail_count` protects
that policy.

# Bug report — 2026-08-19 comprehensive correctness pass

This pass records 30 new issues independently verified through
`docs/inter-agent-comms.md`. Candidates 003, 013, 017, 019, and 035 were
declined; candidates 006, 009, and 016 duplicate concerns already present in
the earlier ledger and are not counted again. All entries below are corrected,
and their named tests remain active regressions.

| ID | Area | Problem | Regression test |
| --- | --- | --- | --- |
| FP-AUDIT-001 | FocalCore | The pipeline cannot represent the canonical encoded Adobe RGB MVP curve domain. | `pipeline::tests::default_pipeline_uses_the_adobe_rgb_mvp_working_contract` |
| FP-AUDIT-002 | FocalCore | The enabled tonal-curve module is parameterless and does no processing. | `pipeline::tests::a_non_identity_tonal_curve_changes_the_rendered_pixel` |
| FP-AUDIT-004 | FocalCore | Output transform publishes negative or above-one display values. | `pipeline::tests::output_transform_bounds_encoded_display_channels` |
| FP-AUDIT-005 | FocalCore scope | Empty buffers with zero dimensions are accepted. | `scope::tests::invalid_analysis_boundaries_are_rejected` |
| FP-AUDIT-007 | FocalPlot | Completed asynchronous analysis may remain unpolled without another repaint. | `app::tests::pending_analysis_keeps_event_polling_alive` |
| FP-AUDIT-008 | FocalPlot | PNG eXIf orientation is ignored. | `loader::tests::png_exif_orientation_is_applied_before_scope_analysis` |
| FP-AUDIT-010 | FocalCurve | Cancellation raised by the final progress callback still publishes a preview. | `pipeline::tests::final_progress_cancellation_does_not_publish_a_preview` |
| FP-AUDIT-011 | FocalPlot | Rendering a public zero-resolution scope can underflow or panic. | `vectorscope::tests::trace_rendering_rejects_invalid_public_analysis_shapes` |
| FP-AUDIT-012 | FocalPlot | Scope rendering trusts density/colour lengths and can panic. | `vectorscope::tests::trace_rendering_rejects_invalid_public_analysis_shapes` |
| FP-AUDIT-014 | FocalPlot | Reverse-highlight cancellation is checked only once per row. | `vectorscope::tests::a_wide_single_row_reverse_scan_observes_cancellation` |
| FP-AUDIT-015 | FocalPlot | Scrolling unrelated UI changes the scope search radius. | `app::tests::scope_scroll_is_ignored_when_the_scope_is_not_hovered` |
| FP-AUDIT-018 | FocalCurve | Decode discards alpha before applying the decided transparency policy. | `pipeline::tests::transparent_input_requires_confirmation_and_can_flatten_over_white` |
| FP-AUDIT-020 | FocalCurve | EXIF ColorSpace parsing ignores the TIFF count field. | `pipeline::tests::malformed_exif_colour_space_count_is_rejected` |
| FP-AUDIT-021 | FocalCurve | Public source/prepared dimensions can disagree with their buffers. | `pipeline::tests::preparation_rejects_inconsistent_and_non_finite_source_pixels` |
| FP-AUDIT-022 | FocalCurve | Superseded image decode/preparation monopolises the worker. | `loader::tests::an_active_obsolete_request_does_not_block_a_new_request` |
| FP-AUDIT-023 | FocalCurve | Export errors are silent and a fixed relative path may be overwritten. | `pipeline::tests::explicit_export_destination_reports_filesystem_errors` |
| FP-AUDIT-024 | FocalCurve | Luma scaling clips channels independently and changes their ratios before output conversion. | `curve::tests::luma_adjustment_preserves_channel_ratios_before_output_conversion` |
| FP-AUDIT-025 | FocalCurve | Mutable derivative points can violate established invariants. | `curve::tests::derivative_point_updates_reject_non_finite_values` |
| FP-AUDIT-026 | FocalCurve | Bezier handles can make segment X non-monotonic while evaluation assumes monotonicity. | `curve::tests::bezier_segment_handles_remain_ordered_on_x` |
| FP-AUDIT-027 | FocalCore | Display-encoded `Image` values are not constrained to their bounded contract. | `image::tests::encoded_contracts_are_bounded_but_linear_contracts_are_not` |
| FP-AUDIT-028 | FocalCore scope | Scope analysis accepts unlabelled bytes and silently assumes sRGB. | `scope::tests::scope_analysis_requires_an_explicit_srgb_byte_contract` |
| FP-AUDIT-029 | FocalCore scope | Forward scope analysis cannot cooperatively cancel. | `scope::tests::forward_and_reverse_scans_observe_preexisting_cancellation` |
| FP-AUDIT-030 | FocalPlot | Trace intensity and sharpness accept non-finite values. | `vectorscope::tests::trace_rendering_rejects_non_finite_presentation_parameters` |
| FP-AUDIT-031 | FocalCurve | Luma choices omit the required Adobe RGB coefficients. | `curve::tests::adobe_rgb_luma_uses_the_project_coefficients` |
| FP-AUDIT-032 | FocalCore scope | Reverse-highlight analysis cannot cooperatively cancel. | `scope::tests::forward_and_reverse_scans_observe_preexisting_cancellation` |
| FP-AUDIT-033 | FocalCurve | Preparation accepts non-finite source pixels and can quantise NaN to plausible bytes. | `pipeline::tests::preparation_rejects_inconsistent_and_non_finite_source_pixels` |
| FP-AUDIT-034 | FocalCore scope | Independently fitted RGB↔RYB splines are not an inverse pair between knots. | `scope::tests::ryb_mapping_round_trips_between_knots` |
| FP-AUDIT-036 | FocalCurve | Curve documentation incorrectly calls the Adobe RGB domain sRGB-like. | `curve::tests::curve_domain_contract_names_adobe_rgb` |
| FP-AUDIT-037 | FocalCurve | Histograms use Rec. 709 coefficients for encoded Adobe RGB samples. | `pipeline::tests::histograms_use_adobe_rgb_luma_for_the_canonical_curve_domain` |
| FP-AUDIT-038 | FocalCurve | Invalidating a preview does not immediately cancel its active worker. | `preview::tests::invalidating_a_preview_signals_its_active_cancellation_token` |

# Bug report — 2026-08-20 Phase One completion

The Phase One completion pass corrected the following editor defects. The named tests are active regressions in the ordinary workspace suite.

| ID | Severity | Confirmed defect | Regression test |
| --- | --- | --- | --- |
| FP-EDITOR-007 | High | A completion from the previous photograph could be accepted after opening a new photograph, briefly re-enabling export for stale pixels. | `app::tests::opening_another_image_invalidates_in_flight_preview_and_export_state` |
| FP-EDITOR-008 | High | Zoom enlarged a fixed one-megapixel proxy instead of resampling the visible full-resolution source region. | `app::tests::zoom_sampling_uses_the_visible_region_and_never_exceeds_one_megapixel`, `preview::tests::preview_sampling_extracts_only_the_requested_source_region` |
| FP-EDITOR-009 | High | Full-resolution export processing and encoding ran synchronously on the GUI thread. | `app::tests::export_requires_pixels_from_the_current_completed_render`, `image_io::tests::png_export_embeds_an_srgb_icc_profile` |
| FP-EDITOR-010 | Medium | FocalEditor used FocalPlot's duplicate numerical scope analyser and active scope work could not be cancelled. | `scope::tests::submitting_a_new_scope_cancels_the_active_scan` |
| FP-EDITOR-011 | Medium | JPEG/PNG orientation and embedded ICC profiles were ignored, and 16-bit alpha was quantised before transparency detection. | `image_io::tests::png_orientation_is_applied_exactly_once_at_decode`, `image_io::tests::embedded_adobe_rgb_profile_enters_core_with_an_adobe_contract`, `image_io::tests::sixteen_bit_alpha_is_not_quantised_before_transparency_detection` |
| FP-EDITOR-012 | Medium | Re-entering crop editing could display the previously cropped texture under source-relative handles, and export could apply an unconfirmed crop. | `app::tests::crop_is_excluded_from_render_snapshot_until_finalised`, `preview::tests::applied_crop_sampling_maps_the_visible_crop_region_back_to_the_source` |
| FP-EDITOR-013 | Low | Failed thumbnail requests remained permanently marked as requested and could never retry. | `app::tests::failed_thumbnail_decode_becomes_retryable` |

Interactive preview preparation, histograms, processing, scopes, and export now stay off the GUI thread. Preview sampling uses the visible source region, native source detail where available, and a one-megapixel upper bound; full-resolution processing is reserved for export.
