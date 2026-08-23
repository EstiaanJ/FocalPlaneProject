---
aliases:
  - Bugs
  - Known bugs
  - Regression tests
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# Defect and regression ledger

This ledger records confirmed project-wide defects and the active regression tests retained after correction. New audits should append a dated section; unresolved concerns belong under **Open risks**, not among confirmed defects.

Routine verification remains:

```sh
cargo test -p focal-core
cargo test -p focal-curve
cargo test -p focal-plot
cargo test -p focal-editor
```

## Open risks

These require a decision, shared boundary, or further evidence rather than a bug label:

- FocalCurve's prototype ICC identification is not a substitute for the planned shared `focal-io` colour-management boundary.
- FocalPlot deliberately labels its decoded-sRGB assumption but does not yet interpret arbitrary embedded profiles in the standalone harness.
- FocalCurve calculates Adobe RGB luma histograms correctly, but its current UI label still says “Rec. 709 luminance”.
- The 150 ms cancellation target still needs measurement on the target system.

## Recurring patterns and lessons

Most historical defects occurred at boundaries: asynchronous ownership, image meaning, invalid state, incorrect routing, and visible behaviour which numerical tests alone could not judge. Make those contracts explicit, reject invalid state early, test complete state transitions, and retain repeatable human checks for visible work. The actionable strategy and review checklist live in [[Testing]].

## 2026-08-19 project audit

These original audit defects are fixed. FP-PLOTS-001 is internally consistent in the experimental harness; the product contract remains confirmation and flattening at the shared opaque-image boundary.

| ID | Area | Defect and correction | Active regression test |
| --- | --- | --- | --- |
| FP-CORE-001 | FocalCore | Unknown pipeline versions rendered with current semantics; they are now rejected explicitly without development migrations. | `pipeline::tests::unsupported_pipeline_version_is_rejected` |
| FP-CORE-002 | FocalCore | Parameter validity depended on resulting pixels; module parameters are now validated independently. | `pipeline::tests::non_finite_parameters_are_rejected_even_when_the_pixel_result_is_finite` |
| FP-CURVE-001 | FocalCurve | Adobe RGB values were clipped into sRGB before the editable curve; wide-gamut distinctions now survive until the output transform. | `pipeline::tests::wide_gamut_values_remain_distinct_until_after_the_editable_curve` |
| FP-PLOTS-001 | FocalPlot | Forward and reverse analysis treated semi-transparent colour differently; the harness is consistent and production uses an opaque boundary. | `vectorscope::tests::semi_transparent_pixels_use_the_same_colour_in_analysis_and_reverse_highlighting` |
| FP-PLOTS-002 | FocalPlot | RYB hue knots used piecewise-linear interpolation instead of the documented spline. | `vectorscope::tests::ryb_hue_mapping_has_a_continuous_slope_at_internal_knots` |
| FP-PLOTS-003 | FocalPlot | JPEG EXIF orientation was ignored before display and analysis. | `loader::tests::jpeg_exif_orientation_is_applied_before_scope_analysis` |

### Follow-up boundary and interaction defects

| ID | Area | Defect | Active regression test |
| --- | --- | --- | --- |
| FP-CORE-003 | FocalCore | Cancellation raised by an initial or final progress callback could be reported as success. | `pipeline::tests::cancellation_during_initial_progress_cancels_an_empty_pipeline`; `pipeline::tests::cancellation_during_empty_completion_progress_does_not_report_success`; `pipeline::tests::cancellation_during_final_progress_does_not_report_success` |
| FP-CORE-004 | FocalCore | Invalid white-balance state was accepted before the decoded-image controls were consolidated as bounded Warmth and Tint parameters. | `pipeline::tests::out_of_range_white_balance_adjustments_are_rejected`; `module::tests::adjustment_validation_rejects_non_finite_percentage_values` |
| FP-PLOTS-004 | FocalPlot | Scope mapping accepted out-of-domain coordinates and silently clamped negative reverse-search radii. | `vectorscope::tests::linear_source_coordinates_reject_values_outside_display_domain`; `vectorscope::tests::reverse_highlight_rejects_negative_radius` |
| FP-PLOTS-005 | FocalPlot | Reverse searches started from hover, could not capture clicks, and did not cancel active scans. | `app::tests::scope_hover_does_not_start_reverse_search`; `app::tests::scope_panels_capture_clicks_without_enabling_dragging`; `loader::tests::cancelled_highlight_does_not_emit_a_stale_result` |

## 2026-08-19 Focal Editor filmstrip review

| ID | Severity | Defect | Active regression test |
| --- | --- | --- | --- |
| FP-EDITOR-001 | High | Selecting a sibling rebuilt the filmstrip and discarded every thumbnail. | `selecting_a_sibling_does_not_rebuild_an_unchanged_filmstrip` |
| FP-EDITOR-002 | Medium | The thumbnail worker decoded obsolete queued directories. | `thumbnail_worker_drops_queued_requests_from_obsolete_directories` |
| FP-EDITOR-003 | High | Preview requests could invalidate an unrelated image load. | `preview_requests_do_not_invalidate_an_in_flight_image_load` |
| FP-EDITOR-004 | High | Export could use an older preview after edits or source selection changed. | `export_requires_pixels_from_the_current_completed_render` |
| FP-EDITOR-005 | High | Main-image loading replaced a small thumbnail with a full-resolution GPU texture. | `loading_main_image_does_not_replace_cached_filmstrip_thumbnail` |
| FP-EDITOR-006 | Medium | Completed thumbnail work did not keep UI polling alive. | `pending_thumbnails_keep_ui_polling_after_other_work_finishes` |

Filmstrip loading is demand-driven and protected by `filmstrip_prefetches_twice_the_visible_thumbnail_count`.

## 2026-08-19 comprehensive correctness pass

Thirty new issues were independently confirmed and corrected. Candidates 003, 013, 017, 019, and 035 were declined; 006, 009, and 016 duplicated existing concerns. The remaining identifiers deliberately preserve the audit numbering.

| ID | Area | Defect | Active regression test |
| --- | --- | --- | --- |
| FP-AUDIT-001 | FocalCore | The pipeline could not represent the canonical encoded Adobe RGB MVP curve domain. | `pipeline::tests::default_pipeline_uses_the_adobe_rgb_mvp_working_contract` |
| FP-AUDIT-002 | FocalCore | The enabled tonal-curve module had no parameters or effect. | `pipeline::tests::a_non_identity_tonal_curve_changes_the_rendered_pixel` |
| FP-AUDIT-004 | FocalCore | Output transform published out-of-range display values. | `pipeline::tests::output_transform_bounds_encoded_display_channels` |
| FP-AUDIT-005 | FocalCore scope | Empty buffers with zero dimensions were accepted. | `scope::tests::invalid_analysis_boundaries_are_rejected` |
| FP-AUDIT-007 | FocalPlot | Completed asynchronous analysis could remain unpolled. | `app::tests::pending_analysis_keeps_event_polling_alive` |
| FP-AUDIT-008 | FocalPlot | PNG eXIf orientation was ignored. | `loader::tests::png_exif_orientation_is_applied_before_scope_analysis` |
| FP-AUDIT-010 | FocalCurve | Final-progress cancellation still published a preview. | `pipeline::tests::final_progress_cancellation_does_not_publish_a_preview` |
| FP-AUDIT-011/012 | FocalPlot | Public scope rendering trusted invalid resolution and buffer shapes and could panic. | `vectorscope::tests::trace_rendering_rejects_invalid_public_analysis_shapes` |
| FP-AUDIT-014 | FocalPlot | Wide single-row reverse scans did not check cancellation often enough. | `vectorscope::tests::a_wide_single_row_reverse_scan_observes_cancellation` |
| FP-AUDIT-015 | FocalPlot | Scrolling unrelated UI changed reverse-search radius. | `app::tests::scope_scroll_is_ignored_when_the_scope_is_not_hovered` |
| FP-AUDIT-018 | FocalCurve | Decode discarded alpha before the decided transparency boundary. | `pipeline::tests::transparent_input_requires_confirmation_and_can_flatten_over_black_in_linear_light` |
| FP-AUDIT-020 | FocalCurve | EXIF ColorSpace parsing ignored the TIFF count. | `pipeline::tests::malformed_exif_colour_space_count_is_rejected` |
| FP-AUDIT-021 | FocalCurve | Public source dimensions could disagree with their buffers. | `pipeline::tests::preparation_rejects_inconsistent_and_non_finite_source_pixels` |
| FP-AUDIT-022 | FocalCurve | Obsolete decode and preparation monopolised the worker. | `loader::tests::an_active_obsolete_request_does_not_block_a_new_request` |
| FP-AUDIT-023 | FocalCurve | Export errors were silent and a fixed path could be overwritten. | `pipeline::tests::explicit_export_destination_reports_filesystem_errors` |
| FP-AUDIT-024 | FocalCurve | Luma adjustment clipped channels independently and changed their ratios before output conversion. | `curve::tests::luma_adjustment_preserves_channel_ratios_before_output_conversion` |
| FP-AUDIT-025 | FocalCurve | Mutable derivative points could violate curve invariants. | `curve::tests::derivative_point_updates_reject_non_finite_values` |
| FP-AUDIT-026 | FocalCurve | Bezier handles could make X non-monotonic while evaluation assumed monotonicity. | `curve::tests::bezier_segment_handles_remain_ordered_on_x` |
| FP-AUDIT-027 | FocalCore | Display-encoded images were not bounded to their declared contract. | `image::tests::encoded_contracts_are_bounded_but_linear_contracts_are_not` |
| FP-AUDIT-028 | FocalCore scope | Scope analysis accepted unlabelled bytes and silently assumed sRGB. | `scope::tests::scope_analysis_requires_an_explicit_srgb_byte_contract` |
| FP-AUDIT-029/032 | FocalCore scope | Forward and reverse scope scans could not cooperatively cancel. | `scope::tests::forward_and_reverse_scans_observe_preexisting_cancellation` |
| FP-AUDIT-030 | FocalPlot | Trace presentation accepted non-finite intensity and sharpness. | `vectorscope::tests::trace_rendering_rejects_non_finite_presentation_parameters` |
| FP-AUDIT-031 | FocalCurve | Luma mode omitted the required Adobe RGB coefficients. | `curve::tests::adobe_rgb_luma_uses_the_project_coefficients` |
| FP-AUDIT-033 | FocalCurve | Preparation accepted non-finite source pixels. | `pipeline::tests::preparation_rejects_inconsistent_and_non_finite_source_pixels` |
| FP-AUDIT-034 | FocalCore scope | Independently fitted RGB↔RYB splines were not inverse between knots. | `scope::tests::ryb_mapping_round_trips_between_knots` |
| FP-AUDIT-036 | FocalCurve | Curve documentation named the Adobe RGB domain as sRGB-like. | `curve::tests::curve_domain_contract_names_adobe_rgb` |
| FP-AUDIT-037 | FocalCurve | Histograms used Rec. 709 coefficients for encoded Adobe RGB values. | `pipeline::tests::histograms_use_adobe_rgb_luma_for_the_canonical_curve_domain` |
| FP-AUDIT-038 | FocalCurve | Invalidating a preview did not immediately cancel its worker. | `preview::tests::invalidating_a_preview_signals_its_active_cancellation_token` |

## 2026-08-20 Phase One completion

| ID | Severity | Defect | Active regression test |
| --- | --- | --- | --- |
| FP-EDITOR-007 | High | A completion from the previous photograph could re-enable export for stale pixels. | `app::tests::opening_another_image_invalidates_in_flight_preview_and_export_state` |
| FP-EDITOR-008 | High | Zoom enlarged a fixed proxy instead of sampling the visible full-resolution region. | `app::tests::zoom_sampling_uses_the_visible_region_and_never_exceeds_one_megapixel`; `preview::tests::preview_sampling_extracts_only_the_requested_source_region` |
| FP-EDITOR-009 | High | Full-resolution export processing and encoding ran on the GUI thread. | `app::tests::export_requires_pixels_from_the_current_completed_render`; `image_io::tests::png_export_embeds_an_srgb_icc_profile` |
| FP-EDITOR-010 | Medium | Focal Editor used duplicate scope analysis and could not cancel active work. The reusable worker now lives in FocalPlot. | `focal-plot scope::tests::submitting_a_new_scope_cancels_the_active_scan` |
| FP-EDITOR-011 | Medium | Orientation and ICC profiles were ignored, and 16-bit alpha was quantised before transparency detection. | `image_io::tests::png_orientation_is_applied_exactly_once_at_decode`; `image_io::tests::embedded_adobe_rgb_profile_enters_core_with_an_adobe_contract`; `image_io::tests::sixteen_bit_alpha_is_not_quantised_before_transparency_detection` |
| FP-EDITOR-012 | Medium | Crop editing could show stale cropped pixels and export an unconfirmed crop. | `app::tests::crop_is_excluded_from_render_snapshot_until_finalised`; `preview::tests::applied_crop_sampling_maps_the_visible_crop_region_back_to_the_source` |
| FP-EDITOR-013 | Low | Failed thumbnail requests could never retry. | `app::tests::failed_thumbnail_decode_becomes_retryable` |

Interactive preparation, histograms, processing, scopes, and export now stay off the GUI thread. Preview processing uses a display-bounded visible source region; export alone uses the untouched full-resolution source.

## 2026-08-21 Phase Two feature review

The Phase Two implementation was reviewed against [[Testing#Feature review checklist]] and the recurring boundary lessons above. These defects were found and fixed before the review was completed:

| ID | Severity | Defect | Correction and active regression test |
| --- | --- | --- | --- |
| FP-EDITOR-014 | High | Loupe and white-balance sampling used the transformed full-image rectangle even when the preview texture represented only a zoomed visible source region. | Both now use the displayed sampled-texture rectangle; `app::tests::loupe_and_pixel_sampling_clamp_at_view_boundaries` covers sampled UVs and out-of-bounds sampling. |
| FP-EDITOR-015 | Medium | The clipping fallback marked any pixel with a zero channel as low-light clipped, so chromatic highlights could be painted as black clipping. | Low-light fallback uses display lightness; `app::tests::clipping_fallback_does_not_mark_chromatic_highlights_as_lowlights` and `pipeline::tests::output_report_preserves_boundary_clipping_before_display_clamping` cover the regression. |
| FP-EDITOR-016 | High | Full-resolution export results had no request identity or cancellation owner and could update editor state after the current image or edit snapshot had changed. | Export requests carry a generation and cancellation token; obsolete requests are cancelled and ignored. `app::tests::a_new_preview_cancels_and_invalidates_an_export_request` covers the state transition. |
| FP-EDITOR-017 | Medium | The processing bar could report Ready while an asynchronous full-resolution export was active. | Export is represented as processing with the same processing colour; `app::tests::processing_bar_state_prioritises_loading_and_clamps_progress` covers loading, preview, export, and ready states. |
| FP-EDITOR-018 | Medium | Image and thumbnail workers had stale-result rejection but no cancellation owner, allowing obsolete TIFF work to continue without a cooperative stop signal. | Load and thumbnail requests now carry cancellation tokens and check them at decode boundaries; `image_io::tests::cancelled_image_boundaries_stop_before_decode_or_flattening` covers the contract. |

The same review added boundary coverage for TIFF orientation, embedded ICC, 16-bit precision, and transparency in `image_io::tests::tiff_orientation_is_applied_exactly_once_at_decode`, `image_io::tests::tiff_embedded_icc_profile_enters_the_canonical_adobe_boundary`, and `image_io::tests::tiff_decode_preserves_sixteen_bit_precision_and_transparency`.
