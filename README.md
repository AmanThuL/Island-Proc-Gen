# Island-Proc-Gen

Procedural volcanic-island generator in Rust. A deterministic simulation pipeline
stitches together geomorphology (stream-power uplift + erosion), hydrology (flow
routing, river extraction), climate (orographic precipitation), and ecology
(biome assignment) on a shared 2D field layer, rendered live with `wgpu` + `egui`
and exportable as CPU-side PNG galleries for headless batch runs.

This is a single-developer research project. It is **pre-alpha** — read
[`PROGRESS.md`](PROGRESS.md) before assuming any feature is real.

## Status

**Sprint 1B climate + ecology closed 2026-04-17.** The full 16-stage
canonical pipeline runs at app startup and populates every promised
field: topography, pit-fill, flow routing, accumulation, basins,
rivers, temperature, precipitation, fog, PET, water balance, soil
moisture, biome weights, and hex aggregation. 12 overlays toggle in
the egui panel (6 Sprint 1A geomorph + 6 Sprint 1B climate /
ecology / hex). The wind-direction slider in the Params panel re-
runs the climate-ecology chain via `pipeline.run_from(StageId::Precipitation)`
and refreshes overlay textures on the same frame — propagation
guarded at the test level by
`sim::validation_stage::tests::wind_dir_rerun_propagates_through_biome_chain`.
**270 tests** across 8 crates. `cargo run -p app` opens a working
window on macOS / Metal.

**Sprint 1A render + sim pipeline shipped 2026-04-14 / 2026-04-15.**
8 sim stages, 4 pipeline-tail validation invariants (Sprint 1B adds
4 more, total 8), canonical 8-colour palette locked against
`assets/visual/palette_reference.jpg`, real Viridis / Turbo /
Categorical / TerrainHeight / BinaryBlue lookup tables, three
camera presets (Hero / Top Debug / Low Oblique), Calinou CC0
blue-noise dither, `shaders/terrain.wgsl` + `shaders/overlay.wgsl`
combining the §3.2 visual package.

**Sprint 0 (scaffolding) shipped 2026-04-13.** Workspace foundation
— 8 crates, `WorldState` three-layer split, winit + wgpu + egui
shell with orbit / pan / zoom camera.

See [`PROGRESS.md`](PROGRESS.md) for the full acceptance-checklist
status, commit-level breakdown, and deferred-to-later-sprints list.

## Quick start

```bash
# Prerequisite: Rust stable, edition 2024 (rustc >= 1.85)
cargo build --workspace
cargo test  --workspace
cargo run   -p app       # opens a local winit window
```

Controls in the app window:
- Left-drag — orbit
- Right-drag — pan
- Scroll — zoom
- Close window — clean exit

## Layout

| Crate | Role |
|---|---|
| `crates/core` | Pure-CPU state: `WorldState`, `ScalarField2D<T>`, `Seed`, `SimulationPipeline`, `validation` (8 invariants), `FLOW_DIR_SINK` / `D8_OFFSETS` / neighborhood constants, `BiomeType` / `BiomeWeights`. Must compile without any graphics crate. |
| `crates/sim` | 16 canonical pipeline stages (Sprint 1A: Topography, Coast, PitFill, DerivedGeomorph, FlowRouting, Accumulation, Basins, Rivers; Sprint 1B: Temperature, Precipitation, FogLikelihood, Pet, WaterBalance, SoilMoisture, BiomeWeights, HexProjection) + tail `ValidationStage`. `StageId` enum locks pipeline indices for `SimulationPipeline::run_from`. |
| `crates/hex` | `HexGrid` + axis-aligned box tessellation (v1 simplification; Sprint 5 can refit to true hexagonal Voronoi). |
| `crates/data` | Built-in presets (`volcanic_single`, `volcanic_twin`, `caldera`), golden-seed snapshots, `SummaryMetrics` regression tiers. |
| `crates/gpu` | `wgpu` device/surface management, depth attachment. |
| `crates/render` | Descriptor-based `OverlayRegistry` (12 overlays), `TerrainRenderer`, `OverlayRenderer`, `SkyRenderer`, canonical palette, camera-preset LUT math. All `&'static str` field-key dispatch confined to `overlay.rs`. |
| `crates/ui` | `egui` panels — overlay toggles, camera controls, preset params (+ Sprint 1B wind slider), stats. |
| `crates/app` | `winit` event loop, orbit camera, preset loading, save/load Path wrapper, slider → `run_from` wiring. |

Crate deps flow strictly one way: `app → render → gpu → core` and
`app → ui/sim/data → core`. `core` is a sink; nothing below it in the graph.

## Documentation

- [`docs/design/island_generation_complete_roadmap.md`](docs/design/island_generation_complete_roadmap.md)
  — the big-picture roadmap and architectural rules.
- [`docs/design/sprints/`](docs/design/sprints/) — per-sprint implementation
  plans and acceptance checklists.
- [`docs/papers/`](docs/papers/) — indexed paper knowledge base (Core Pack +
  per-sprint add-ons).
- [`CLAUDE.md`](CLAUDE.md) — context for Claude Code / Sonnet / Opus sessions.
- [`PROGRESS.md`](PROGRESS.md) — sprint status and roadmap dashboard.

## License

MIT OR Apache-2.0 (per `[workspace.package]` in the root `Cargo.toml`).
