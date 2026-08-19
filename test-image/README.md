---
title: FocalPlane Test Images
tags:
  - project/testing
---

# FocalPlane Test Images

Provenance: these fixtures were supplied directly by the project owner for FocalPlane testing on 2026-07-30. They are approved project test inputs. If an externally sourced fixture is added later, record its source and licence here before distribution.

Tracked images must be no larger than 10 kB (10,000 bytes). Larger local stress fixtures are intentionally ignored by Git.

| File | Purpose | Repository status |
| --- | --- | --- |
| `psf.png` | Point-spread-function response | Tracked |
| `neutral_gray.png` | Neutrality and identity checks | Tracked |
| `color_patches.png` | Colour transform and patch comparisons | Tracked |
| `test.png` | Small RGBA PNG decode fixture | Tracked |
| `gradients.png` | Gradient continuity, curves, and banding | Tracked |
| `slanted-edge-check.png` | Slanted-edge visual reference | Tracked |
| `slanted_edge_mtf.png` | Slanted-edge MTF input | Tracked |
| `test.jpg` | Small JPEG decode fixture | Tracked |
| `mtf.png` | General MTF input | Tracked |
| `pure_chroma.png` | Chroma isolation and saturation | Tracked |
| `frequency_sweep_mtf.png` | Frequency response sweep | Tracked |
| `radial_mtf.png` | Large radial MTF stress input | Local only |
| `full_range.png` | Large full-range gradient stress input | Local only |
| `pure_chroma_16.png` | 16-bit PNG precision stress input | Local only |
| `large-image-test.jpg` | Large JPEG performance and memory input | Local only |

The non-default EXIF-orientation fixture is generated deterministically in `crates/focal-engine/tests/fixture_policy.rs` by adding orientation 6 metadata to `test.jpg`. Keeping it in test code avoids another binary fixture while providing a reproducible rotated-orientation input.
