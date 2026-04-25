# Island-Proc-Gen

Procedural volcanic-island generator in Rust. A deterministic simulation pipeline
stitches together geomorphology (stream-power uplift + erosion), hydrology (flow
routing, river extraction), climate (orographic precipitation, water balance,
fog), and ecology (biome assignment) on a shared 2D field layer, rendered live
with `wgpu` + `egui` and exportable as CPU-side PNG galleries for headless batch
runs.

This is a single-developer research project. It is **pre-alpha** — APIs, data
formats, and visual output will change without notice. Read
[`PROGRESS.md`](PROGRESS.md) for the current state before assuming any feature
is stable.

![Island-Proc-Gen preview — `volcanic_caldera_young` seed 42, HexOnly view](assets/screenshots/hero.png)

*Sprint 3.5 hex-surface base read on `volcanic_caldera_young` seed 42:
true-hex axial-offset tessellation, tonal-ramp elevation cue at the
volcanic peak, dominant-biome fill, 5-class coast edge decoration at
the rim, and the DD3 river polyline grammar fanning out from the
summit to the sea. Captured via `--headless` from the
`sprint_3_5_hex_surface/` baseline.*

## Status

A 19-stage canonical simulation pipeline (18 `StageId` variants + terminal
`ValidationStage`) runs at app startup, populating continuous 2D fields for
terrain height, slope, flow routing, drainage basins, rivers, temperature,
precipitation, fog, potential evapotranspiration, soil moisture, biome weights,
sediment thickness, deposition flux, fog water input, and per-hex
aggregation (incl. `HexCoastClass` and `HexRiverCrossing`) on a 256×256 grid.
Twenty live overlays toggle in the egui panel with per-descriptor alpha
sliders; a `Continuous / HexOverlay / HexOnly` ViewMode selector
(Sprint 3.5: `HexOnly` is now an intentional map mode rather than a debug
heatmap — tonal-ramp elevation + biome fill + edge-decoration coast
classes + continuous river polylines, no overlays required), a hex-pick
inspector panel, wind-direction / climate / erosion / SPACE-lite sliders,
and a World-panel preset / seed / aspect picker all re-run the relevant
slice of the pipeline and refresh overlay textures on the same frame.
`cargo run -p app` opens a working window on macOS / Metal.

A windowless `--headless` harness drives the same pipeline for batch capture
and deterministic regression: `cargo run -p app -- --headless <request.ron>`
writes an artifact tree of overlay PNGs + `SummaryMetrics` + a top-level
`summary.ron`; `--headless-validate <run> --against <expected>` diffs two
summaries by blake3 hash and exits with `0` / `2` / `3` for pass / pipeline-
regression / tool-error. Five checked-in baselines live at
`crates/data/golden/headless/{sprint_1a_baseline,sprint_1b_acceptance,sprint_2_erosion,sprint_3_sediment_climate,sprint_3_5_hex_surface}/`
— Sprint 2's before/after erosion pairs use `schema_v2 preset_override`;
Sprint 3.5's 27-shot baseline uses `schema_v3` to add per-shot `view_mode`
(3 archetypes × 3 seeds × 3 view modes).

Nothing is stabilised yet: preset parameters, field semantics, save-file
format, and the visual package all remain fluid. The project has no binary
releases, no wasm build, and no web viewer. The end-goal shape is a deterministic
batch generator (seed + preset → PNG gallery + hex-aggregated JSON) plus a live
viewer for research exploration.

## Quick start

```bash
# Prerequisite: Rust stable, edition 2024 (rustc >= 1.85)
cargo build --workspace
cargo test  --workspace
cargo run   -p app       # opens a local winit window

# Headless batch capture (no window):
cargo run -p app --release -- \
    --headless crates/data/golden/headless/sprint_1a_baseline/request.ron

# Regression diff of a run against a checked-in baseline:
cargo run -p app --release -- \
    --headless-validate /captures/headless/<run_id>/ \
    --against crates/data/golden/headless/sprint_1a_baseline/
```

Controls in the app window:
- Left-drag — orbit
- Right-drag — pan
- Scroll — zoom
- Left-click (no drag) on the viewport — pick hex; selection lands in the `HexInspect` dock tab (Sprint 3.5 DD7)
- Wind-dir / climate / erosion / SPACE-lite sliders in the Params panel — re-run the affected slice of the pipeline
- World panel — preset / seed / aspect / sea-level controls (full or partial regen depending on which knob)
- Camera panel `View Mode` dropdown — `Continuous / HexOverlay / HexOnly`
- Close window — clean exit, persists dock layout

## Layout

| Crate | Role |
|---|---|
| `crates/core` | Pure-CPU state: `WorldState`, `ScalarField2D<T>`, `Seed`, `SimulationPipeline`, `validation` (directorised by family in Sprint 3.4 — `validation/{hydro,climate,erosion,biome,hex}.rs` — and grew through Sprint 3.5 with `hex_coast_class_well_formed` + `coastal_margin_sm_floor_applied` + `cloud_forest_f_t_envelope_matches_sprint_3_5_lock`), `FLOW_DIR_SINK` / `D8_OFFSETS` / neighborhood constants, `BiomeType` / `BiomeWeights`, `CoastType` (5 classes + `Unknown` sentinel), `HexCoastClass` (Sprint 3.5 DD4: 7 classes), `ErosionBaseline`, `HexDebugAttributes` + `HexRiverCrossing` (6-edge encoding since Sprint 3.5.B). Must compile without any graphics crate. |
| `crates/sim` | 19-stage canonical pipeline (18 `StageId` variants: geomorph + hydro + `ErosionOuterLoop` + `CoastType` + climate + ecology + hex projection; + terminal `ValidationStage`). `StageId` enum locks pipeline indices for `SimulationPipeline::run_from`. Sprint 3.5 adds `hex_coast_class.rs` classifier (consumes the persisted `coast_fetch_integral` from `CoastTypeStage` — no duplicate raycast). |
| `crates/hex` | `HexGrid` + flat-top hex geometry (Sprint 3.5.A DD2: true-hex odd-r-offset Voronoi assignment; `geometry.rs` is the single source of truth for `axial_to_pixel` / `pixel_to_axial` / `offset_to_pixel` / `pixel_to_offset` / 6-edge `HexEdge` enum). |
| `crates/data` | Built-in presets (`volcanic_single`, `volcanic_twin`, `volcanic_twin_old`, `caldera`, `volcanic_caldera_young`, `volcanic_eroded_ridge`), golden-seed snapshots, `SummaryMetrics` regression tiers (1A core + 1B climate/ecology + Sprint 2 erosion/coast-type fields + Sprint 3.5 DD8 `hex_attrs_hash` / `hex_debug_river_crossing_hash` / `hex_coast_class_hash`), and the five `--headless` baselines under `golden/headless/`. |
| `crates/gpu` | `wgpu` device/surface management, depth attachment, headless `Surface::None` path for `--headless`. |
| `crates/render` | Descriptor-based `OverlayRegistry` (Sprint 3 `sprint_3_defaults()` returns 20 descriptors), `TerrainRenderer`, `OverlayRenderer` (per-frame alpha uniform writes), `SkyRenderer`, `HexSurfaceRenderer` (Sprint 3.5: procedural unit-hex VB + per-instance buffer with tonal-ramp elevation + biome fill + 5-class coast edge tint), `HexRiverRenderer` (Sprint 3.5: edge-to-edge polyline keyed by DD3 `RiverWidth` bucket), canonical palette (incl. `PaletteId::CoastType` and `HEX_EDGE_*`), `ValueRange::LogCompressedClampPercentile` for long-tail overlays, camera-preset LUT math. The `overlay/` module was directorised in Sprint 3.4 — all `&'static str` field-key dispatch is confined to `overlay/resolve.rs`. |
| `crates/ui` | `egui` panels — overlay toggles, camera controls, preset params (with wind-direction / climate / erosion / SPACE-lite sliders), stats. |
| `crates/app` | `winit` event loop, orbit camera, preset loading, save/load Path wrapper, slider → `run_from` wiring, the `--headless` / `--headless-validate` harness (capture request parsing, CPU truth bake, GPU offscreen beauty, summary diff), `egui_dock` layout with `HexInspectPanel` (Sprint 3.5 DD7: read-only attr table for the click-picked hex; `Continuous / HexOverlay / HexOnly` ViewMode dispatched via `render_stack_for(ViewMode)` to keep interactive and headless render paths bit-identical). |

Crate deps flow strictly one way: `app → render → gpu → core` and
`app → ui/sim/data → core`. `core` is a sink; nothing below it in the graph.

## Documentation

- [`docs/architecture/ARCHITECTURE.md`](docs/architecture/ARCHITECTURE.md)
  — system architecture, data model, pipeline walkthrough, the eight
  hard invariants, and the headless harness.
- [`PROGRESS.md`](PROGRESS.md) — milestone dashboard and development history.
- [`CLAUDE.md`](CLAUDE.md) — operating notes for AI coding agents working
  in this repo.
- [`crates/data/golden/headless/README.md`](crates/data/golden/headless/README.md)
  — layout of the `--headless-validate` baselines + author workflow for
  regenerating them.
- [`docs/papers/README.md`](docs/papers/README.md) — indexed paper
  knowledge base (Core Pack + per-topic add-ons).

## License

Dual-licensed under either of

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual-licensed as above, without any additional terms
or conditions.
