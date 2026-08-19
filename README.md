# FocalPlane

FocalPlane is a rewrite of a Rust photo-editing project, built primarily for the owner's personal photographic workflow. The current goal is a fast, correct, standalone editor with powerful global controls and no required catalogue.

The retained `OLD_EDITOR` source is a GUI reference only. Its processing pipeline and film-photography model are not part of this rewrite.

Start with [`docs/README.md`](docs/README.md), then read:

- [`docs/FocalPlane.md`](docs/FocalPlane.md) — product purpose and values;
- [`docs/Architecture Decisions.md`](docs/Architecture%20Decisions.md) — settled architectural decisions;
- [`docs/Clean Architecture Migration.md`](docs/Clean%20Architecture%20Migration.md) — consolidation instructions;
- [`docs/Engineering Principles.md`](docs/Engineering%20Principles.md) — quality and human-decision rules;
- [`docs/Bug Report.md`](docs/Bug%20Report.md) — confirmed defects and regression tests.

Do not begin implementing Focal Editor until the human owner supplies and documents the new GUI description.
