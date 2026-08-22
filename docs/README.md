---
aliases:
  - Documentation
  - Docs home
  - Start here
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# FocalPlane documentation

Start with [[FocalPlane]] for the purpose, values, application boundaries, and intended workflow.

- [[Architecture Decisions]] — settled processing, colour, I/O, curve, cancellation, and saved-state decisions
- [[Clean Architecture Migration]] — instructions for consolidating the experiments into one architecture
- [[MVP]] — prototype target and post-MVP exclusions
- [[Focal Editor & Focal Core]] — editor and rendering responsibilities
- [[Focal Core Pipeline]] — modular processing order and preview execution
- [[Presets and Saved Edits]] — editable documents, exports, presets, and copied settings
- [[Sliders]] — controls and interaction principles
- [[Decoded Image Corrections]] — decoded-image white balance, local contrast, and denoising research
- [[FocalLib]] — later library-management workflow
- [[Testing]] — correctness and reproducibility
- [[RAW Rendering Reference Capture]] — a human test guide for paired RAW and camera-JPEG references
- [[Accelerated Rendering Visual Checklist]] — human parity and artefact checks for optimized CPU and GPU rendering
- [[Engineering Principles]] — development standards and human-directed decisions
- [[Vectorscope Research]] — darktable-inspired colour-scope algorithm and visual design
- [[Bug Report]] — confirmed defects and executable regression tests
- [[Project Audits]] — chronological project-wide reviews, human responses, and outcomes
- [[Open Questions]] — unresolved product decisions

## Documentation tags

Tags describe a document's provenance and intended readers; they do not change its authority. In particular, a machine-authored summary does not turn an unresolved colour or product choice into an agent-owned decision.

- `authorship/human` — written primarily by the human owner
- `authorship/machine` — written primarily by an LLM coding agent
- `authorship/mixed` — materially written or revised by both
- `audience/human` — intended for the human owner or human testers
- `audience/agents` — implementation guidance or context for LLM coding agents
- `status/archive` — retained as historical evidence, not a source of current instructions

A document may have both audience tags. Archived notes should point to the current decision, status, or defect document where one exists. Existing collaborative documents are tagged `authorship/mixed`; [[MVP]] is intentionally unchanged in accordance with the owner's standing instruction not to edit it.
