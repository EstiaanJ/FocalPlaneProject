# Focal Editor

The first standalone Focal Editor GUI slice follows [`docs/Focal-Editor Old GUI.md`](../../docs/Focal-Editor%20Old%20GUI.md): open one PNG or JPEG directly, show Before and After previews, adjust exposure and contrast, and keep a histogram visible. It uses egui/eframe and performs image loading and preview rendering away from the UI thread.

Run it with:

```text
cargo run -p focal-editor -- path/to/photo.jpg
```

The current prototype saves editable parameters to a versioned JSON sidecar and exports an 8-bit sRGB PNG. Its decoder currently supplies FocalCore with the decoded sRGB contract; the shared `focal-io` boundary and the canonical Adobe RGB MVP path remain the next colour-management integration work.

The curve editor, crop controls, old response curve, and library workflow are intentionally absent. FocalPlot scopes are reserved for integration after their numerical analysis is extracted behind the shared boundary.

See [`../../docs/Focal Editor & Focal Core.md`](../../docs/Focal%20Editor%20%26%20Focal%20Core.md) and [`AGENTS.md`](AGENTS.md) for the architectural and development constraints.
