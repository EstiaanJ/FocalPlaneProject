---
title: FocalPlane Test Images
tags:
  - project/testing
  - authorship/mixed
  - audience/human
  - audience/agents
---

# FocalPlane Test Images

Provenance: these fixtures were supplied directly by the project owner for FocalPlane testing on 2026-07-30. They are approved local project test inputs. If an externally sourced fixture is added later, record its source and licence here before distribution.

All image fixtures are currently ignored by Git and remain local. This file records the expected local fixture names and their purposes; it does not imply that the files are distributed with the repository.

| File | Purpose |
| --- | --- |
| `psf.png` | Point-spread-function response |
| `neutral_gray.png` | Neutrality and identity checks |
| `color_patches.png` | Colour transform and patch comparisons |
| `test.png` | Small RGBA PNG decode fixture |
| `gradients.png` | Gradient continuity, curves, and banding |
| `slanted-edge-check.png` | Slanted-edge visual reference |
| `slanted_edge_mtf.png` | Slanted-edge MTF input |
| `test.jpg` | Small JPEG decode fixture |
| `mtf.png` | General MTF input |
| `pure_chroma.png` | Chroma isolation and saturation |
| `frequency_sweep_mtf.png` | Frequency response sweep |
| `radial_mtf.png` | Large radial MTF stress input |
| `full_range.png` | Large full-range gradient stress input |
| `pure_chroma_16.png` | 16-bit PNG precision stress input |
| `large-image-test.jpg` | Large JPEG performance and memory input |

Non-default JPEG and PNG orientation fixtures are generated deterministically inside the relevant crate tests. Keeping them in test code avoids additional binary fixtures while providing reproducible rotated-orientation inputs.

## Local X-T5 rendering references

`X-T5_RAW/` contains owner-supplied, ignored RAW/JPEG reference material. The current reference pair is `PROVIA_JPG.RAF` and `PROVIA_JPG.JPG`; it is intended for relative comparison with the camera's Provia/Standard rendering, not colourimetric calibration. See [[RAW Rendering Reference Capture]] before adding captures or annotations.
