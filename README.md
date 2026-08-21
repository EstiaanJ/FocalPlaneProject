# FocalPlane

FocalPlane is a rewrite of a Rust photo-editing project, built primarily for the owner's personal photographic workflow. The current goal is a fast, correct, standalone editor with powerful global controls and no required catalogue.

Start with [`docs/README.md`](docs/README.md), then read:

- [`docs/FocalPlane.md`](docs/FocalPlane.md) — product purpose and values;
- [`docs/Architecture Decisions.md`](docs/Architecture%20Decisions.md) — settled architectural decisions;
- [`docs/Clean Architecture Migration.md`](docs/Clean%20Architecture%20Migration.md) — consolidation instructions;
- [`docs/Engineering Principles.md`](docs/Engineering%20Principles.md) — quality and human-decision rules;
- [`docs/Bug Report.md`](docs/Bug%20Report.md) — confirmed defects and regression tests.

## Current folder structure

The tracked project structure is shown below. Generated `target/` directories and local tool metadata are omitted.

```text
FocalPlaneProject/
├── AGENTS.md
├── Cargo.toml                 # Workspace definition
├── README.md
├── crates/
│   ├── better-plots/          # FocalPlot visual harness
│   │   └── src/
│   ├── exposure-cruve-tool/   # FocalCurve visual harness
│   │   ├── assets/
│   │   └── src/
│   ├── focal-core/            # GUI-independent production processing
│   │   ├── src/
│   │   └── tests/
│   └── focal-editor/          # Standalone desktop editor
│       ├── examples/
│       └── src/
├── docs/                      # Product, architecture, testing, and research notes
│   ├── README.md
│   ├── FocalPlane.md
│   ├── Architecture Decisions.md
│   ├── Clean Architecture Migration.md
│   ├── Engineering Principles.md
│   ├── Testing.md
│   └── Bug Report.md
└── test-image/                # Controlled images and RTSet comparisons
    ├── README.md
    └── RTSet/
```
