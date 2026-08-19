---
aliases:
  - Folder Structure
  - Project folders
  - Repository structure
---

# Folder Structure

This note distinguishes the current repository from the intended long-term layout. The tree is still changing rapidly, so documentation should not pretend that planned locations already exist.

## Current repository

### `crates/focal-editor`

The Focal Editor application. Its initial GUI description is documented in [[Focal-Editor Old GUI]]; keep the first implementation narrow and do not carry over the old processing model.

### `crates/focal-core`

The core image-processing pipeline shared by FocalPlane applications.

### `crates/exposure-cruve-tool`

The current experimental curve application. `cruve` is a historical spelling mistake. Its canonical product/documentation name is **FocalCurve**, and its eventual Rust package and folder name is `focal-curve`.

### `crates/better-plots`

The current experimental plotting application. Its canonical product/documentation name is **FocalPlot**, and its eventual Rust package and folder name is `focal-plot`.

FocalCurve and FocalPlot should remain independently runnable harnesses for focused visual experimentation while also providing reusable widgets. Their image-processing semantics must move toward shared FocalCore and `focal-io` boundaries rather than becoming alternative production pipelines.

### `data`

Application data during development. Prototype JSON sidecars and other editable state may begin here. In future, appropriate data may live in the user's home directory or FocalLib database.

### `docs`

Project documentation and the Obsidian vault.

### `OLD_EDITOR`

Source from the old Focal Plane editor, renamed Focal Editor in this project. This code is retained as a rough GUI reference and to avoid needless rewriting. It is not the architectural basis for the new processing pipeline, and its film-photography concepts do not carry over.

### `test-image`

Test images for verification and controlled processing tests.

## Intended application layout

### `apps/focal-editor`

Focal Editor may eventually live as an application here rather than under `crates`.

### `crates/focal-io`

The planned shared file boundary for decoding, input-profile interpretation, orientation, metadata, alpha handling, output colour conversion, and encoding. This is not a second processing pipeline: it prepares explicit image buffers for FocalCore and writes FocalCore results.

### `crates/focal-curve`

The canonical future name for FocalCurve. It remains a standalone experiment and supplies a reusable curve widget for Focal Editor.

### `crates/focal-plot`

The canonical future name for FocalPlot. It remains a standalone experiment and supplies reusable scope widgets for Focal Editor.

## External folders

### `/home/estiaan/code/FocalPlane`

The predecessor project. This repository is a rewrite.

### `/home/estiaan/code/Reference_Projects`

Reference open-source photo editors such as Filmulator, darktable, RawTherapee, and Spektrafilm.

## Related documentation

- [[FocalPlane]] — project overview and rewrite status
- [[Focal Editor & Focal Core]] — editor and processing responsibilities
- [[MVP]] — current development scope
- [[Architecture Decisions]] — binding architectural and colour-domain decisions
- [[Clean Architecture Migration]] — instructions for reaching the intended layout safely
