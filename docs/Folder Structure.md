---
aliases:
  - Folder Structure
  - Project folders
  - Repository structure
---

# Folder Structure

The repository is still changing rapidly. Current and planned locations are distinguished below; planned paths must not be described as though they already exist.

## Current repository

```text
crates/
  focal-core/           production processing architecture
  focal-editor/         standalone desktop editor
  exposure-cruve-tool/  FocalCurve experimental harness
  better-plots/         FocalPlot experimental harness
data/                   development data and prototype edit state
docs/                   project documentation and Obsidian vault
OLD_EDITOR/             GUI reference from the predecessor project
test-image/             controlled test images
```

`cruve` is a historical spelling mistake. Use **FocalCurve** in product documentation and `focal-curve` for the eventual package and folder name. Likewise, **FocalPlot** is the product name and `focal-plot` is the intended package and folder name.

FocalCurve and FocalPlot remain independently runnable visual harnesses and may supply reusable widgets. Validated processing semantics belong in FocalCore; shared decoding, profiles, metadata, orientation, transparency handling, output conversion, and encoding belong at the planned `focal-io` boundary.

## OLD_EDITOR

`OLD_EDITOR` is only a high-level GUI reference. Its processing pipeline and film-photography model do not carry into this rewrite.

## Intended layout

Focal Editor may eventually move to `apps/focal-editor`. The intended shared libraries are:

- `crates/focal-io` — file and metadata boundaries;
- `crates/focal-curve` — standalone curve harness and reusable widget;
- `crates/focal-plot` — standalone scope harness and reusable widgets.

Renames and moves should be isolated mechanical changes rather than mixed with processing work.

## External folders

- `/home/estiaan/code/FocalPlane` — predecessor project;
- `/home/estiaan/code/Reference_Projects` — local references including darktable, RawTherapee, Filmulator, and Spektrafilm.

See [[Architecture Decisions]] for binding boundaries and [[Clean Architecture Migration]] for the remaining consolidation work.
