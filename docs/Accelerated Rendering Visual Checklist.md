---
aliases:
  - GPU visual checklist
  - Accelerated rendering review
tags:
  - authorship/machine
  - audience/human
---

# Accelerated rendering visual checklist

Use this checklist to compare the Reference, optimized CPU, and GPU renderers.
This is a human quality check, not an automated test result. Record the date,
commit, operating system, CPU/thread count, GPU adapter and driver, display, and
whether the build is Preview or Export quality.

## Controlled images

- [ ] Compare `color_patches.png` for hue shifts, saturation changes, neutral contamination, and clipping in highly chromatic patches.
- [ ] Compare `neutral_gray.png` for colour casts and changes in neutral gradients.
- [ ] Compare `gradients.png` for banding, discontinuities, posterisation, and endpoint clipping.
- [ ] Compare `pure_chroma_16.png` for channel clipping and unexpected hue rotation.
- [ ] Compare `frequency_sweep_mtf.png`, `slanted_edge_mtf.png`, and `radial_mtf.png` for ringing, edge changes, aliasing, and directional artefacts.
- [ ] Compare at least one ordinary photograph and one high-chroma photograph rather than relying only on synthetic fixtures.

## Viewing procedure

- [ ] Toggle Reference and optimized CPU results at fit-to-view, 100%, and 200%.
- [ ] Toggle Reference and GPU results at the same zoom levels without rescaling one result differently.
- [ ] Inspect shadows, midtones, highlights, saturated colours, neutrals, smooth gradients, fine texture, and hard edges.
- [ ] Check both neutral settings and representative non-zero Exposure and White Balance settings.
- [ ] When an accelerated spatial stage is added, check its minimum, typical, and maximum approved settings independently and in combination.
- [ ] Confirm Preview and Export preserve the same adjustment meaning; note expected scale-dependent differences separately.
- [ ] Drag controls repeatedly and confirm there are no stale frames, flashes of an older render, or visible partial results after cancellation.

## Record

- Date:
- Commit:
- CPU and optimized thread count:
- GPU adapter and driver:
- Display and scaling:
- Fixtures and photographs checked:
- Differences observed:
- Accepted differences and corresponding numerical tolerance:
- Result: Pass / Fail / Needs investigation
- Tester:
