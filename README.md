# FocalPlane

FocalPlane is a rewrite of a Rust photo-editing project, built primarily for the owner's personal photographic workflow. The current goal is a fast, correct, standalone editor with powerful global controls and no required catalogue.

Start with [`docs/README.md`](docs/README.md), then read:

- [`docs/FocalPlane.md`](docs/FocalPlane.md) — product purpose and values;
- [`docs/Architecture Decisions.md`](docs/Architecture%20Decisions.md) — settled architectural decisions;
- [`docs/Clean Architecture Migration.md`](docs/Clean%20Architecture%20Migration.md) — consolidation instructions;
- [`docs/Engineering Principles.md`](docs/Engineering%20Principles.md) — quality and human-decision rules;
- [`docs/Bug Report.md`](docs/Bug%20Report.md) — confirmed defects and regression tests.

## Current folder structure

The working project structure is summarised below. Generated `target/` directories, local tool metadata, and most individual documentation and fixture files are omitted.

```text
FocalPlaneProject/
├── AGENTS.md
├── Cargo.toml                 # Workspace definition
├── README.md
├── crates/
│   ├── focal-plot/            # FocalPlot visual harness and reusable scope widgets
│   │   └── src/
│   ├── focal-curve/           # FocalCurve visual harness
│   │   ├── assets/
│   │   └── src/
│   ├── focal-core/            # GUI-independent production processing
│   │   ├── src/
│   │   └── tests/
│   ├── focal-io/              # Shared decoding and encoding boundary
│   │   └── src/
│   └── focal-editor/          # Standalone desktop editor
│       ├── examples/
│       └── src/
├── docs/                      # Product, architecture, testing, and research notes
└── test-image/                # Local controlled images and RTSet comparisons
```
