# Island-Proc-Gen

Procedural volcanic-island generator in Rust. A deterministic simulation pipeline
stitches together geomorphology (stream-power uplift + erosion), hydrology (flow
routing, river extraction), climate (orographic precipitation), and ecology
(biome assignment) on a shared 2D field layer, rendered live with `wgpu` + `egui`
and exportable as CPU-side PNG galleries for headless batch runs.

This is a single-developer research project. It is **pre-alpha** — read
[`PROGRESS.md`](PROGRESS.md) before assuming any feature is real.

## Status

**Sprint 1A render-shell library shipped 2026-04-15**, building on the
sim pipeline that landed 2026-04-14. The whole simulation chain runs at
app startup and the entire non-window render shell is in place:
canonical 8-colour palette locked against
`assets/visual/palette_reference.jpg`, real Viridis / Turbo / Categorical
/ TerrainHeight / BinaryBlue lookup tables, three camera presets
(Hero / Top Debug / Low Oblique), a blue-noise loader wired to the
Calinou CC0 textures at `assets/noise/`, the `build_terrain_mesh` /
`build_sea_quad` mesh builders, `shaders/terrain.wgsl` combining the
§3.2 A1/A2/A4 visual package, and all 6 Sprint 1A overlays pointing
at the real `derived.*` fields.

Only the final wiring remains: swap Sprint 0's placeholder `TerrainRenderer`
for the new mesh + WGSL pipeline, hand the 6 `OverlayDescriptor`s to a
CPU-side overlay render path, and capture the 9 baseline screenshots
(3 camera presets × 3 golden seeds). All of those need a confirmed
`cargo run -p app` window-session pass. Until then `cargo run -p app`
still draws the Sprint 0 rainbow quad.

See [`PROGRESS.md`](PROGRESS.md) for the full acceptance-checklist
status.

**Sprint 0 (scaffolding) shipped 2026-04-13.** The workspace boots, the
`WorldState` three-layer split is in place, and a placeholder rainbow quad
renders through the winit + wgpu + egui shell with orbit/pan/zoom.

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
| `crates/core` | Pure-CPU state: `WorldState`, `ScalarField2D<T>`, `Seed`, `SimulationPipeline`, `validation`, `FLOW_DIR_SINK` / `D8_OFFSETS` / neighborhood constants. Must compile without any graphics crate. |
| `crates/sim` | Sprint 1A sim stages (Topography, Coast, PitFill, DerivedGeomorph, FlowRouting, Accumulation, Basins, Rivers) + `ValidationStage`. Climate + ecology land in Sprint 1B. |
| `crates/hex` | Hex aggregation (Sprint 1B+). |
| `crates/data` | Built-in presets (`volcanic_single`, `volcanic_twin`, `caldera`) and golden seeds. |
| `crates/gpu` | `wgpu` device/surface management. |
| `crates/render` | Descriptor-based `OverlayRegistry` + placeholder terrain mesh. |
| `crates/ui` | `egui` panels (overlay toggles, preset params, stats). |
| `crates/app` | `winit` event loop, orbit camera, save/load Path wrapper. |

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
