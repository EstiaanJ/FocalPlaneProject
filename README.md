# FocalPlane

FocalPlane is a photo-editing project, built primarily for the owner's personal photographic workflow. The current goal is a fast, correct, standalone editor with powerful global controls and no required catalogue.


## Build from source

Install the stable [Rust toolchain](https://www.rust-lang.org/tools/install), then run these commands from the repository root:

```sh
cargo build --workspace --release
```

The standalone editor is written to `target/release/focal-editor`. To build and run it directly with an image:

```sh
cargo run --release -p focal-editor -- path/to/photo.jpg
```

Or just 

```sh
cargo run --release -p focal-editor
```

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
